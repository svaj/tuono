use syn::{Attribute, ItemFn, ReturnType, Type, TypePath};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TuonoMacro {
    Api,
    Handler,
    Middleware,
}
impl TuonoMacro {
    pub fn as_str(&self) -> &'static str {
        match self {
            TuonoMacro::Api => "api",
            TuonoMacro::Handler => "handler",
            TuonoMacro::Middleware => "middleware",
        }
    }
}

#[allow(dead_code)]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum InternalTuonoMacro {
    Type,
}
#[allow(dead_code)]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum AllTuonoMacros {
    TuonoMacros(TuonoMacro),
    InternalTuonoMacros(InternalTuonoMacro),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TuonoFunction {
    ApiHandler,
    Handler,
    Middleware,
    RouterGenertor,
}

impl TuonoFunction {
    pub fn as_str(&self) -> &'static str {
        match self {
            TuonoFunction::ApiHandler => TuonoMacro::Api.as_str(),
            TuonoFunction::Handler => TuonoMacro::Handler.as_str(),
            TuonoFunction::Middleware => TuonoMacro::Middleware.as_str(),
            TuonoFunction::RouterGenertor => "router_generator",
        }
    }

    // Given an array of syn::Attribute, returns true if the segments are "tuono_lib" and "middleware" for example
    pub fn has_macro_attr(attrs: &[Attribute], macro_attr: TuonoMacro) -> bool {
        attrs.iter().any(|attr| {
            let path = attr.path();

            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            segments == ["tuono_lib", macro_attr.as_str()]
        })
    }

    fn compare_return_type_ignore_generics(item: &ItemFn, expected_type_name: &str) -> bool {
        // 1. Get return type from signature
        if let ReturnType::Type(_, ty) = &item.sig.output {
            // 2. Look for TypePath (e.g., std::vec::Vec)
            if let Type::Path(TypePath { path, .. }) = &**ty {
                // 3. Get the last segment, which is the type name
                if let Some(last_segment) = path.segments.last() {
                    // 4. Compare ident ("Vec") and ignore arguments ("<...>")
                    return last_segment.ident.to_string() == expected_type_name;
                }
            }
        }
        false
    }

    pub fn is_middleware_fn(item_fn: &ItemFn) -> bool {
        TuonoFunction::has_macro_attr(&item_fn.attrs, TuonoMacro::Middleware)
    }
    pub fn is_handler_fn(item_fn: &ItemFn) -> bool {
        TuonoFunction::has_macro_attr(&item_fn.attrs, TuonoMacro::Handler)
    }
    pub fn is_api_handler_fn(item_fn: &ItemFn) -> bool {
        TuonoFunction::has_macro_attr(&item_fn.attrs, TuonoMacro::Api)
    }

    // Given an ItemFn, checks its return type to see if its a Router / implemented all the traits of a router
    pub fn is_router_fn(item_fn: &ItemFn) -> bool {
        // can't get return type :(
        let ReturnType::Type(_rarrow, _box_type) = &item_fn.sig.output else {
            // see if return type is Router
            return false;
        };
        return TuonoFunction::compare_return_type_ignore_generics(item_fn, &"Router");
    }

    #[allow(dead_code)]
    pub fn get_value_from_attr(&self, attrs: &[Attribute]) -> &str {
        let crate_name = env!("CARGO_PKG_NAME");
        attrs
            .iter()
            .find(|attr| {
                let path = attr.path();
                let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
                if segments.len() <= 2 {
                    return false;
                }
                segments[0] == crate_name && TuonoFunction::try_from(segments[1].clone()).is_ok()
            })
            .map_or("", |_| self.as_str())
    }
}

impl std::convert::TryFrom<ItemFn> for TuonoFunction {
    type Error = &'static str;

    fn try_from(func: ItemFn) -> Result<Self, Self::Error> {
        if TuonoFunction::is_middleware_fn(&func) {
            return Ok(TuonoFunction::Middleware);
        } else if TuonoFunction::is_api_handler_fn(&func) {
            return Ok(TuonoFunction::ApiHandler);
        } else if TuonoFunction::is_handler_fn(&func) {
            return Ok(TuonoFunction::Handler);
        } else if TuonoFunction::is_router_fn(&func) {
            return Ok(TuonoFunction::RouterGenertor);
        }
        return Err("Unknown function");
    }
}

impl std::convert::TryFrom<String> for TuonoFunction {
    type Error = ();

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "api" => Ok(TuonoFunction::ApiHandler),
            "handler" => Ok(TuonoFunction::Handler),
            "middleware" => Ok(TuonoFunction::Middleware),
            "router_generator" => Ok(TuonoFunction::RouterGenertor),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::module_functions::TuonoFunction;

    #[test]
    fn test_is_middleware_fn() {
        // Create a dummy function with the middleware attribute
        let func: syn::ItemFn = syn::parse_quote!(
            #[tuono_lib::middleware]
            fn test_fn() {}
        );
        assert!(TuonoFunction::is_middleware_fn(&func));

        // Create a dummy function with a different attribute
        let func2: syn::ItemFn = syn::parse_quote!(
            #[other_attr]
            fn test_fn() {}
        );
        assert!(!TuonoFunction::is_middleware_fn(&func2));
    }
    #[test]
    fn test_is_handler_fn() {
        // Create a dummy function with the middleware attribute
        let func: syn::ItemFn = syn::parse_quote!(
            #[tuono_lib::handler]
            fn test_fn() {}
        );
        assert!(TuonoFunction::is_handler_fn(&func));

        // Create a dummy function with a different attribute
        let func2: syn::ItemFn = syn::parse_quote!(
            #[other_attr]
            fn test_fn() {}
        );
        assert!(!TuonoFunction::is_handler_fn(&func2));
    }
}
