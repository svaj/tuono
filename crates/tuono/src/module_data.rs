use quote::quote;
use std::path::Path;
use std::sync::{Arc, Mutex};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Attribute, Expr, FnArg, Ident, Item, ItemFn, ReturnType, Type, TypePath, parse_quote};

use crate::macros::TuonoMacros;

pub const DEFAULT_ROUTER_STR: &str = "Router::new()";

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DebugItemFn {
    pub fn_call_str: String,
    pub is_router_fn: bool, // TODO: remove or replace with enum type of available TuonoLib defined functions
                            // TODO: add argument recievers / types to match if available when calling?
                            // TODO: worry about adding generics support or matching traits
                            // TODO: ensure function is publically available. in the rust module
}

impl DebugItemFn {
    pub fn get_fn_call_to_str(item: &ItemFn) -> String {
        let mut arguments: Punctuated<FnArg, Comma> = Punctuated::new();
        let mut passed_arguments: Punctuated<Expr, Comma> = Punctuated::new();
        for (i, arg) in item.sig.inputs.iter().enumerate() {
            if let FnArg::Typed(pat_type) = arg {
                let arg_name = Ident::new(&format!("arg_{}", i), item.sig.ident.span());
                let arg_type = &pat_type.ty;
                let argument: FnArg = parse_quote!(#arg_name: #arg_type);
                arguments.push(argument);
                passed_arguments.push(parse_quote!(#arg_name));
            }
        }

        let sig_ident_str = &item.sig.ident.to_string();
        let with_args_str = quote!(#passed_arguments).to_string();

        format!("{sig_ident_str}({with_args_str})")
    }
}

impl From<ItemFn> for DebugItemFn {
    fn from(item: ItemFn) -> Self {
        return DebugItemFn {
            fn_call_str: DebugItemFn::get_fn_call_to_str(&item),
            is_router_fn: ModuleData::is_router_fn(&item),
        };
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleData {
    pub full_path: String,
    // todo restrict to .rs files
    // todo resolve base crate path, or relative to where we started. (maybe this goes in RouteDirectoryInfo?)
    // todo ensure generation of router path supports wildcards  & double check extractor work works (maybe this is in RouteDirectoryInfo)
    //TODO add type enum to get fns by type
    // todo add helper function to handle dealing with lock arc/mutex nonsense.
    pub middlewares: Arc<Mutex<Vec<DebugItemFn>>>, // Todo verify substates work
    pub routers: Arc<Mutex<Vec<DebugItemFn>>>,
    #[allow(dead_code)]
    pub handlers: Arc<Mutex<Vec<DebugItemFn>>>,
    #[allow(dead_code)]
    pub api_handlers: Arc<Mutex<Vec<DebugItemFn>>>,
    // pub other_data: todo - get list of other functions/structs/enums etc from public modules.  Services?
}

impl ModuleData {
    pub fn new(path_str: &String) -> Option<Self> {
        if !(std::fs::exists(path_str).unwrap_or_default()) {
            return None;
        }
        let (middlewares, routers, api_handlers, handlers) =
            ModuleData::read_module_methods_from_file(&path_str);

        Some(ModuleData {
            full_path: path_str.clone(),
            middlewares,
            routers,
            handlers,
            api_handlers,
        })
    }

    pub fn has_middlewares(&self) -> bool {
        !self.middlewares.lock().unwrap().is_empty()
    }
    pub fn has_routers(&self) -> bool {
        !self.routers.lock().unwrap().is_empty()
    }
    #[allow(dead_code)]
    pub fn has_handlers(&self) -> bool {
        !self.api_handlers.lock().unwrap().is_empty()
    }
    #[allow(dead_code)]
    pub fn has_api_handlers(&self) -> bool {
        !self.handlers.lock().unwrap().is_empty()
    }

    pub fn get_pathed_module_use_str(&self) -> String {
        let path = &self.full_path;
        let module_import = self.get_module_import();
        format!(
            r#"#[path="{path}"]
        mod {module_import};
        "#
        )
    }

    //todo add fn get_relative_module_use_str

    pub fn get_module_import(&self) -> String {
        let Some(extension) = Path::new(&self.full_path)
            .extension()
            .unwrap_or_default()
            .to_str()
        else {
            return "".to_string();
        };
        let with_dot = ".".to_string() + extension;
        self.full_path
            .as_str()
            .to_string()
            .replace(&with_dot, "")
            .replace('/', "_")
            .replace('.', "_dot_")
            .replace('-', "_hyphen_")
            .to_lowercase()
    }
    pub fn generate_router(&self, ending_semicolon: bool) -> String {
        let semicolon_str = if ending_semicolon { ";" } else { "" };

        // Lock the mutex once and keep the guard for the entire scope
        let routers_lock = self.routers.lock().unwrap();

        // Check if the vector is empty before accessing elements
        if routers_lock.is_empty() {
            return format!("{DEFAULT_ROUTER_STR}{semicolon_str}");
        }

        // Get the first router and check for multiple entries
        let first_router = &routers_lock[0];
        if routers_lock.len() > 1 {
            println!(
                "Warning: more than one router method in module ({}) found, using first one ({} )",
                self.full_path, first_router.fn_call_str
            );
        }

        let mut router: String = String::new();
        for (i, router_fn) in routers_lock.iter().enumerate() {
            let fn_ident = router_fn.fn_call_str.clone();
            if i > 0 {
                router = format!("{router}.merge({fn_ident})");
            } else {
                router = fn_ident
            }
        }

        if !ending_semicolon {
            return router;
        }
        format!("{router};")
    }

    // Given an array of syn::Attribute, returns true if the segments are "tuono_lib" and "middleware" for example
    pub fn has_macro_attr(attrs: &[Attribute], macro_attr: TuonoMacros) -> bool {
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
        ModuleData::has_macro_attr(&item_fn.attrs, TuonoMacros::Middleware)
    }
    pub fn is_handler_fn(item_fn: &ItemFn) -> bool {
        ModuleData::has_macro_attr(&item_fn.attrs, TuonoMacros::Handler)
    }
    pub fn is_api_handler_fn(item_fn: &ItemFn) -> bool {
        ModuleData::has_macro_attr(&item_fn.attrs, TuonoMacros::Api)
    }

    // Given an ItemFn, checks its return type to see if its a Router / implemented all the traits of a router
    pub fn is_router_fn(item_fn: &ItemFn) -> bool {
        // can't get return type :(
        let ReturnType::Type(_rarrow, _box_type) = &item_fn.sig.output else {
            // see if return type is Router
            return false;
        };
        return ModuleData::compare_return_type_ignore_generics(item_fn, &"Router");
    }

    // Reads a file and returns a Vector of Strings representing functions that are TuonoMacros or Router generating functions
    pub fn read_module_methods_from_file(
        path: &str,
    ) -> (
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
    ) {
        // todo refactor to have one locked vector of module methods, by type maybe?
        let mut middlewares = Vec::new();
        let mut router_fns = Vec::new();
        let mut api_handlers = Vec::new();
        let mut handlers = Vec::new();
        let Ok(file) = fs_extra::file::read_to_string(path) else {
            return (
                Arc::new(Mutex::new(middlewares)),
                Arc::new(Mutex::new(router_fns)),
                Arc::new(Mutex::new(api_handlers)),
                Arc::new(Mutex::new(handlers)),
            );
        };
        let Ok(syntax) = syn::parse_file(&file) else {
            return (
                Arc::new(Mutex::new(middlewares)),
                Arc::new(Mutex::new(router_fns)),
                Arc::new(Mutex::new(api_handlers)),
                Arc::new(Mutex::new(handlers)),
            );
        };

        for item in syntax.items {
            if let Item::Fn(func) = item {
                if ModuleData::is_middleware_fn(&func) {
                    middlewares.push(DebugItemFn::from(func));
                } else if ModuleData::is_api_handler_fn(&func) {
                    api_handlers.push(DebugItemFn::from(func));
                } else if ModuleData::is_handler_fn(&func) {
                    handlers.push(DebugItemFn::from(func));
                } else if ModuleData::is_router_fn(&func) {
                    router_fns.push(DebugItemFn::from(func));
                }
            }
        }
        return (
            Arc::new(Mutex::new(middlewares)),
            Arc::new(Mutex::new(router_fns)),
            Arc::new(Mutex::new(api_handlers)),
            Arc::new(Mutex::new(handlers)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_has_middlewares() {
        let dir_info = ModuleData {
            middlewares: Arc::new(Mutex::new(vec![DebugItemFn {
                fn_call_str: "middleware1(app_state:AppState)".to_string(),
                is_router_fn: false,
            }])),
            ..Default::default()
        };
        assert!(dir_info.has_middlewares());
    }

    #[test]
    fn test_get_middleware_module_import() {
        let module_info = ModuleData {
            full_path: "/some/path/src/routes/middlewares.rs".to_string(),
            ..Default::default()
        };
        // Assuming base path is current dir, but this might vary
        // For test, we can check the format
        let import = module_info.get_module_import();
        assert!(import.ends_with("_middlewares"));
    }

    #[test]
    fn test_middleware_data_new() {
        let temp_dir = TempDir::new().unwrap();
        let middlewares_file = temp_dir.path().join("middlewares.rs");
        let mut file = File::create(&middlewares_file).unwrap();
        writeln!(file, "#[tuono_lib::middleware]\nfn test_middleware() {{}}").unwrap();

        let middleware_data =
            ModuleData::new(&middlewares_file.to_string_lossy().to_string()).unwrap();
        let middlewares = middleware_data.middlewares.lock().unwrap();
        assert_eq!(
            middlewares.as_slice(),
            [DebugItemFn {
                fn_call_str: "test_middleware()".to_string(),
                is_router_fn: false,
            }]
        );
    }

    // TODO Update when adding pub support to ensure a non pub method is not detected, (or is but gives a warning?)
    #[test]
    fn test_has_middleware_attr() {
        // Create a dummy function with the middleware attribute
        let func: syn::ItemFn = syn::parse_quote!(
            #[tuono_lib::middleware]
            fn test_fn() {}
        );
        assert!(ModuleData::is_middleware_fn(&func));

        // Create a dummy function with a different attribute
        let func2: syn::ItemFn = syn::parse_quote!(
            #[other_attr]
            fn test_fn() {}
        );
        assert!(!ModuleData::is_middleware_fn(&func2));
    }

    #[test]
    fn test_read_middleware_methods_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let middlewares_file = temp_dir.path().join("middlewares.rs");
        let mut file = File::create(&middlewares_file).unwrap();
        writeln!(
            file,
            "#[tuono_lib::middleware]\nfn test_middleware() {{}}\nfn other_fn() {{}}"
        )
        .unwrap();
        #[allow(unused)]
        let (methods, router_methods, api_handlers, handlers) =
            ModuleData::read_module_methods_from_file(&middlewares_file.to_string_lossy());
        let middlewares = methods.lock().unwrap();
        let routers = router_methods.lock().unwrap();
        assert_eq!(
            middlewares.as_slice(),
            [DebugItemFn {
                fn_call_str: "test_middleware()".to_string(),
                is_router_fn: false,
            }]
        );
        assert_eq!(routers.len(), 0);
    }
}
