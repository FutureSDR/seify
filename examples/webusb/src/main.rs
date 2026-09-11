#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("run this example with `trunk serve --open`");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::install().expect("install Seify WebUSB controls");
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::rc::Rc;

    use num_complex::Complex32;
    use seify::{
        Args, AsyncRegistry, AsyncRxStreamer, ChannelControls, DynAsyncDevice, DynAsyncRxStreamer,
        Range, RangeItem,
    };
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::spawn_local;
    use web_sys::{
        CanvasRenderingContext2d, Document, Event, HtmlButtonElement, HtmlCanvasElement,
        HtmlInputElement, HtmlSelectElement,
    };

    #[derive(Clone)]
    struct OpenedDevice {
        device: DynAsyncDevice,
        rx_channel: usize,
        controls: ChannelControls,
    }

    type DeviceSlot = Rc<RefCell<Option<OpenedDevice>>>;
    type UiResult<T> = Result<T, String>;

    const READ_TIMEOUT_US: i64 = 1_000_000;

    pub fn install() -> Result<(), JsValue> {
        let device: DeviceSlot = Rc::new(RefCell::new(None));
        set_controls_busy(false, false);

        let open_slot = Rc::clone(&device);
        on_click("open-device", move || {
            set_controls_busy(true, open_slot.borrow().is_some());
            set_status("Waiting for WebUSB device permission…");
            let slot = Rc::clone(&open_slot);
            spawn_local(async move {
                slot.borrow_mut().take();
                match open_device().await {
                    Ok(opened) => {
                        let id = opened
                            .device
                            .id()
                            .await
                            .unwrap_or_else(|_| "unknown device".to_string());
                        let name = format!("{:?} {id}", opened.device.driver());
                        if let Err(error) = populate_controls(&opened).await {
                            set_status(&format!(
                                "Opened {name}, but configuration query failed: {error}"
                            ));
                        } else {
                            set_status(&format!("Opened {name}."));
                        }
                        set_text("device-name", &name);
                        set_panel_visible(true);
                        *slot.borrow_mut() = Some(opened);
                        set_controls_busy(false, true);
                    }
                    Err(error) => {
                        set_status(&format!("Open failed: {error}"));
                        set_panel_visible(false);
                        set_controls_busy(false, false);
                    }
                }
            });
        })?;

        let apply_slot = Rc::clone(&device);
        on_click("apply-config", move || {
            let Some(opened) = apply_slot.borrow().clone() else {
                set_status("Open an SDR first.");
                return;
            };
            set_controls_busy(true, true);
            set_status("Applying receiver configuration…");
            spawn_local(async move {
                match apply_configuration(&opened).await {
                    Ok(()) => set_status("Configuration applied."),
                    Err(error) => set_status(&format!("Configuration failed: {error}")),
                }
                set_controls_busy(false, true);
            });
        })?;

        let capture_slot = Rc::clone(&device);
        on_click("capture", move || {
            let Some(opened) = capture_slot.borrow().clone() else {
                set_status("Open an SDR first.");
                return;
            };
            let sample_count = match integer_input("sample-count") {
                Ok(value) if (1..=262_144).contains(&value) => value,
                Ok(_) => {
                    set_status("Sample count must be between 1 and 262144.");
                    return;
                }
                Err(error) => {
                    set_status(&error);
                    return;
                }
            };
            set_controls_busy(true, true);
            set_status(&format!("Capturing {sample_count} complex samples…"));
            spawn_local(async move {
                match capture_samples(&opened, sample_count).await {
                    Ok(samples) => match plot_magnitude(&samples) {
                        Ok(()) => {
                            set_status(&format!("Captured and plotted {} samples.", samples.len()))
                        }
                        Err(error) => set_status(&format!("Plot failed: {error}")),
                    },
                    Err(error) => set_status(&format!("Capture failed: {error}")),
                }
                set_controls_busy(false, true);
            });
        })?;

        Ok(())
    }

    async fn open_device() -> UiResult<OpenedDevice> {
        let registry = AsyncRegistry::default();
        let args = Args::new();
        registry
            .request_permission(args.clone())
            .await
            .map_err(|error| error.to_string())?;
        let device = registry
            .open_args(args)
            .await
            .map_err(|error| error.to_string())?;
        let capabilities = device
            .capabilities()
            .await
            .map_err(|error| error.to_string())?;
        let rx = capabilities
            .rx_channels
            .into_iter()
            .next()
            .ok_or_else(|| "the opened device has no RX channel".to_string())?;
        Ok(OpenedDevice {
            device,
            rx_channel: rx.channel,
            controls: rx.controls,
        })
    }

    async fn populate_controls(opened: &OpenedDevice) -> UiResult<()> {
        configure_control_panel(&opened.controls)?;
        let rx = opened
            .device
            .rx(opened.rx_channel)
            .await
            .map_err(|error| error.to_string())?;

        if opened.controls.frequency_range.is_some() {
            if let Ok(value) = rx.frequency().value().await {
                set_number("frequency", value);
            }
        }
        if opened.controls.sample_rate_range.is_some() {
            if let Ok(value) = rx.sample_rate().value().await {
                set_number("sample-rate", value);
            }
        }
        if opened.controls.bandwidth_range.is_some() {
            if let Ok(value) = rx.bandwidth().value().await {
                set_number("bandwidth", value);
            } else {
                input("bandwidth")?.set_value("");
            }
        }
        if opened.controls.gain_range.is_some() {
            if let Ok(Some(value)) = rx.gain().value().await {
                set_number("gain", value);
            }
        }
        if let Some(antennas) = &opened.controls.antennas {
            set_select_options("antenna", antennas)?;
            if let Ok(selected) = rx.antenna().selected().await {
                select("antenna")?.set_value(&selected);
            }
        }
        if opened.controls.agc {
            if let Ok(enabled) = rx.agc().enabled().await {
                input("agc")?.set_checked(enabled);
            }
        }
        if opened.controls.dc_offset {
            if let Ok(enabled) = rx.dc_offset().enabled().await {
                input("dc-offset")?.set_checked(enabled);
            }
        }
        Ok(())
    }

    async fn apply_configuration(opened: &OpenedDevice) -> UiResult<()> {
        let rx = opened
            .device
            .rx(opened.rx_channel)
            .await
            .map_err(|error| error.to_string())?;
        if opened.controls.frequency_range.is_some() {
            seify_result(rx.frequency().set(number_input("frequency")?).await)?;
        }
        if opened.controls.sample_rate_range.is_some() {
            seify_result(rx.sample_rate().set(number_input("sample-rate")?).await)?;
        }
        if opened.controls.bandwidth_range.is_some() {
            if let Some(bandwidth) = optional_number_input("bandwidth")? {
                seify_result(rx.bandwidth().set(bandwidth).await)?;
            }
        }
        if opened.controls.gain_range.is_some() {
            seify_result(rx.gain().set(number_input("gain")?).await)?;
        }
        if opened.controls.antennas.is_some() {
            seify_result(rx.antenna().select(&select("antenna")?.value()).await)?;
        }
        if opened.controls.agc {
            seify_result(rx.agc().set_enabled(input("agc")?.checked()).await)?;
        }
        if opened.controls.dc_offset {
            seify_result(
                rx.dc_offset()
                    .set_enabled(input("dc-offset")?.checked())
                    .await,
            )?;
        }
        Ok(())
    }

    async fn capture_samples(
        opened: &OpenedDevice,
        sample_count: usize,
    ) -> UiResult<Vec<Complex32>> {
        let rx = opened
            .device
            .rx(opened.rx_channel)
            .await
            .map_err(|error| error.to_string())?;
        let mut stream = rx.streamer().await.map_err(|error| error.to_string())?;
        stream.activate().await.map_err(|error| error.to_string())?;

        let capture_result = read_exact(&mut stream, sample_count).await;
        let deactivate_result = stream.deactivate().await.map_err(|error| error.to_string());
        let samples = capture_result?;
        deactivate_result?;
        Ok(samples)
    }

    async fn read_exact(
        stream: &mut DynAsyncRxStreamer,
        sample_count: usize,
    ) -> UiResult<Vec<Complex32>> {
        let mut samples = vec![Complex32::default(); sample_count];
        let mut offset = 0;
        while offset < sample_count {
            let read = stream
                .read(&mut [&mut samples[offset..]], READ_TIMEOUT_US)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err(format!(
                    "timed out after {offset} of {sample_count} samples"
                ));
            }
            offset += read;
        }
        Ok(samples)
    }

    fn plot_magnitude(samples: &[Complex32]) -> UiResult<()> {
        if samples.is_empty() {
            return Err("no samples to plot".to_string());
        }
        let canvas: HtmlCanvasElement = element("plot")?;
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")
            .map_err(js_error)?
            .ok_or_else(|| "2D canvas is unavailable".to_string())?
            .dyn_into()
            .map_err(|error| js_error(error.into()))?;
        let width = f64::from(canvas.width());
        let height = f64::from(canvas.height());
        let padding = 24.0;
        let plot_width = width - 2.0 * padding;
        let plot_height = height - 2.0 * padding;
        let count = samples.len() as f64;
        let mean_i = samples.iter().map(|sample| sample.re as f64).sum::<f64>() / count;
        let mean_q = samples.iter().map(|sample| sample.im as f64).sum::<f64>() / count;
        let rms = (samples
            .iter()
            .map(|sample| sample.norm_sqr() as f64)
            .sum::<f64>()
            / count)
            .sqrt();
        let peak = samples
            .iter()
            .map(|sample| sample.norm() as f64)
            .fold(0.0_f64, f64::max);
        // A normalized full-scale complex sinusoid has unit magnitude. Values
        // above one can occur near the square I/Q quantizer corners and clip at
        // the top of this diagnostic plot.
        let full_scale = 1.0;
        let rms_dbfs = 20.0 * (rms / full_scale).max(f64::EPSILON).log10();
        let peak_dbfs = 20.0 * (peak / full_scale).max(f64::EPSILON).log10();
        set_text(
            "plot-summary",
            &format!(
                "Fixed full-scale magnitude · mean I {mean_i:.4}, mean Q {mean_q:.4}, RMS {rms_dbfs:.1} dBFS, peak {peak_dbfs:.1} dBFS"
            ),
        );

        context.set_fill_style_str("#070b0f");
        context.fill_rect(0.0, 0.0, width, height);
        context.set_stroke_style_str("#1e2a35");
        context.set_line_width(1.0);
        for step in 0..=4 {
            let y = padding + plot_height * f64::from(step) / 4.0;
            context.begin_path();
            context.move_to(padding, y);
            context.line_to(width - padding, y);
            context.stroke();
        }

        context.set_stroke_style_str("#43d6a3");
        context.set_line_width(1.25);
        context.begin_path();
        for (index, sample) in samples.iter().enumerate() {
            let x = if samples.len() == 1 {
                padding
            } else {
                padding + plot_width * index as f64 / (samples.len() - 1) as f64
            };
            let magnitude = (sample.norm() as f64 / full_scale).clamp(0.0, 1.0);
            let y = padding + plot_height * (1.0 - magnitude);
            if index == 0 {
                context.move_to(x, y);
            } else {
                context.line_to(x, y);
            }
        }
        context.stroke();
        Ok(())
    }

    fn configure_control_panel(controls: &ChannelControls) -> UiResult<()> {
        let antennas = controls
            .antennas
            .as_ref()
            .is_some_and(|antennas| !antennas.is_empty());
        let fields = [
            ("frequency-field", controls.frequency_range.is_some()),
            ("sample-rate-field", controls.sample_rate_range.is_some()),
            ("bandwidth-field", controls.bandwidth_range.is_some()),
            ("gain-field", controls.gain_range.is_some()),
            ("antenna-field", antennas),
            ("agc-field", controls.agc),
            ("dc-offset-field", controls.dc_offset),
        ];
        for (id, visible) in fields {
            set_element_visible(id, visible);
        }

        if let Some(range) = &controls.frequency_range {
            configure_number_input("frequency", range)?;
        }
        if let Some(range) = &controls.sample_rate_range {
            configure_number_input("sample-rate", range)?;
        }
        if let Some(range) = &controls.bandwidth_range {
            configure_number_input("bandwidth", range)?;
        }
        if let Some(range) = &controls.gain_range {
            configure_number_input("gain", range)?;
        }

        let has_controls = fields.into_iter().any(|(_, visible)| visible);
        set_element_visible("apply-config", has_controls);
        set_element_visible("no-controls", !has_controls);
        set_element_visible("bandwidth-hint", controls.bandwidth_range.is_some());
        Ok(())
    }

    fn configure_number_input(id: &str, range: &Range) -> UiResult<()> {
        let input = input(id)?;
        input.set_min(&range.min().to_string());
        input.set_max(&range.max().to_string());
        let step = match range.items() {
            [RangeItem::Step(_, _, step)] => step.to_string(),
            _ => "any".to_string(),
        };
        input.set_step(&step);
        Ok(())
    }

    fn set_select_options(id: &str, values: &[String]) -> UiResult<()> {
        let select = select(id)?;
        select.set_text_content(None);
        for value in values {
            let option = document()?.create_element("option").map_err(js_error)?;
            option.set_attribute("value", value).map_err(js_error)?;
            option.set_text_content(Some(value));
            select.append_child(&option).map_err(js_error)?;
        }
        Ok(())
    }

    fn on_click(id: &str, handler: impl FnMut() + 'static) -> Result<(), JsValue> {
        let button: HtmlButtonElement = element(id).map_err(|error| JsValue::from_str(&error))?;
        let mut handler = handler;
        let callback = Closure::<dyn FnMut(Event)>::new(move |_| handler());
        button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
        callback.forget();
        Ok(())
    }

    fn set_controls_busy(busy: bool, connected: bool) {
        if let Ok(button) = element::<HtmlButtonElement>("open-device") {
            button.set_disabled(busy);
        }
        for id in ["apply-config", "capture"] {
            if let Ok(button) = element::<HtmlButtonElement>(id) {
                button.set_disabled(busy || !connected);
            }
        }
    }

    fn set_panel_visible(visible: bool) {
        set_element_visible("device-panel", visible);
        set_element_visible("capture-panel", visible);
    }

    fn set_element_visible(id: &str, visible: bool) {
        if let Ok(element) = element::<web_sys::Element>(id) {
            if visible {
                let _ = element.remove_attribute("hidden");
            } else {
                let _ = element.set_attribute("hidden", "");
            }
        }
    }

    fn set_status(message: &str) {
        set_text("status", message);
    }

    fn set_text(id: &str, value: &str) {
        if let Ok(element) = element::<web_sys::Element>(id) {
            element.set_text_content(Some(value));
        }
    }

    fn set_number(id: &str, value: f64) {
        if let Ok(input) = input(id) {
            input.set_value(&format!("{value:.0}"));
        }
    }

    fn integer_input(id: &str) -> UiResult<usize> {
        input(id)?
            .value()
            .parse()
            .map_err(|_| format!("{id} must be a positive integer"))
    }

    fn number_input(id: &str) -> UiResult<f64> {
        input(id)?
            .value()
            .parse()
            .map_err(|_| format!("{id} must be a number"))
    }

    fn optional_number_input(id: &str) -> UiResult<Option<f64>> {
        let value = input(id)?.value();
        if value.trim().is_empty() {
            Ok(None)
        } else {
            value
                .parse()
                .map(Some)
                .map_err(|_| format!("{id} must be a number or empty"))
        }
    }

    fn input(id: &str) -> UiResult<HtmlInputElement> {
        element(id)
    }

    fn select(id: &str) -> UiResult<HtmlSelectElement> {
        element(id)
    }

    fn element<T: JsCast>(id: &str) -> UiResult<T> {
        document()?
            .get_element_by_id(id)
            .ok_or_else(|| format!("missing #{id}"))?
            .dyn_into()
            .map_err(|_| format!("#{id} has the wrong element type"))
    }

    fn document() -> UiResult<Document> {
        web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| "browser document is unavailable".to_string())
    }

    fn js_error(error: JsValue) -> String {
        format!("{error:?}")
    }

    fn seify_result<T>(result: Result<T, seify::Error>) -> UiResult<T> {
        result.map_err(|error| error.to_string())
    }
}
