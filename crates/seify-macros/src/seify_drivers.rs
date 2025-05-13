use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Expr, ExprArray, ExprPath, Ident, ItemEnum, ItemFn, Lit, Meta, MetaList,
    MetaNameValue, Path, Variant, parse_macro_input, spanned::Spanned,
};

fn extract_probefn_ident(attr: &Expr) -> syn::Result<Ident> {
    // println!("Eonfg: {attr:#?}");
    if let Expr::Path(ExprPath {
        path: Path { segments, .. },
        ..
    }) = attr
    {
        if let Some(probefn_ident) = segments.get(0) {
            Ok(probefn_ident.ident.clone())
        } else {
            Err(syn::Error::new(attr.span(), "Invalid Expression #0002"))
        }
    } else {
        Err(syn::Error::new(attr.span(), "Invalid Expression #0001"))
    }
}

fn extract_driver_names(attr: &Expr) -> syn::Result<ExprArray> {
    if let Expr::Array(expr_array) = attr {
        Ok(expr_array.clone())
    } else {
        Err(syn::Error::new(attr.span(), "Invalid Expression #0004"))
    }
}

#[derive(Debug, Default, Clone)]
struct DriverVariantProperties {
    probefn_ident: Option<Ident>,
    driver_strs: Option<ExprArray>,
    driver_cfgs: Option<TokenStream>,
}

pub fn seify_drivers_impl(_attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    // Parse the input enum
    let mut input_enum = syn::parse2::<syn::ItemEnum>(input)?;
    let enum_ident = &input_enum.ident;
    let variants = &mut input_enum.variants;
    // let mut probefn_idents = Vec::new();
    // let mut driver_strs = Vec::new();
    // let mut driver_cfgs = Vec::new();

    let mut driver_properties = Vec::new();

    for variant in variants.iter_mut() {
        let mut new_attrs = Vec::with_capacity(variant.attrs.len());

        let mut driver_property = DriverVariantProperties::default();

        for attr in &variant.attrs {
            match &attr.meta {
                Meta::NameValue(MetaNameValue { path, value, .. }) if path.is_ident("probefn") => {
                    // probefn_idents.push(extract_probefn_ident(value)?);
                    driver_property.probefn_ident = Some(extract_probefn_ident(value)?);
                }
                Meta::NameValue(MetaNameValue { path, value, .. }) if path.is_ident("names") => {
                    // driver_strs.push(extract_driver_names(value)?);
                    driver_property.driver_strs = Some(extract_driver_names(value)?);
                }
                Meta::List(MetaList { path, tokens, .. }) if path.is_ident("cfg") => {
                    // driver_cfgs.push(tokens.clone());
                    driver_property.driver_cfgs = Some(tokens.clone());
                }
                _ => new_attrs.push(attr.clone()),
            }
        }

        if driver_property.probefn_ident.is_none() {
            return Err(syn::Error::new(variant.span(), "No probe function set."));
        }

        driver_properties.push(driver_property);

        // if !names_set {
        //     return Err(syn::Error::new(variant.span(), "No parsing names set"));
        // }

        variant.attrs = new_attrs;
    }

    let mut the_variants_matching = Vec::new();
    // println!("drivers properties: {:#?}", driver_properties);
    for (idk, variant) in driver_properties.into_iter().zip(variants) {
        let driver_cfg = idk.driver_cfgs.unwrap_or_default();
        let driver_probefn = idk.probefn_ident.unwrap();

        let variant_ident = variant.ident.clone();

        let smth = quote! {
            #[cfg(#driver_cfg)]
            {
                if driver.is_none() || matches!(driver, Some(#enum_ident::#variant_ident)) {
                    devs.append(&mut #driver_probefn(&args)?);
                }
            }
            #[cfg(not(#driver_cfg))]
            {
                if matches!(driver, Some(#enum_ident::#variant_ident)) {
                    return Err(Error::FeatureNotEnabled);
                }
            }
        };

        println!("smth: {:#?}", smth);

        the_variants_matching.push(smth);
    }

    // Generate the impl block with the `probe` method
    let expanded = quote! {
            #input_enum

            impl #enum_ident {

     /// Calls each probe function in declaration order and returns concatenated results.
     pub fn enumerate_with_args<A: TryInto<Args>>(a: A) -> Result<Vec<crate::Args>, crate::Error> {

        let args: Args = a.try_into().or(Err(Error::ValueError))?;
        let mut devs = Vec::new();
        let driver = match args.get::<String>("driver") {
            Ok(s) => Some(s.parse::<#enum_ident>()?),
            Err(_) => None,
        };

        #(
            #the_variants_matching
        )*
        Ok(devs)
    }

                fn from_driver_str(s: &str) -> Result<Self, crate::Error> {
                    todo!()
                }
            }
        };

    Ok(expanded.into())
}
