use quote::quote;
use std::path::Path;
use std::sync::{Arc, Mutex};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Expr, FnArg, Ident, Item, ItemFn, parse_quote};

use crate::module_functions::TuonoFunction;

pub const DEFAULT_ROUTER_STR: &str = "Router::new()";

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DebugItemFn {
    pub fn_call_str: String,
    pub type_of_fn: TuonoFunction,
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

impl TryFrom<ItemFn> for DebugItemFn {
    type Error = &'static str;
    fn try_from(item: ItemFn) -> Result<Self, Self::Error> {
        let Ok(type_of_fn) = TuonoFunction::try_from(item.clone()) else {
            return Err("Unknown function");
        };
        return Ok(Self {
            // todo, impl tryfrom for pattern for get_fn_call_to_str
            fn_call_str: DebugItemFn::get_fn_call_to_str(&item),
            type_of_fn,
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleData {
    pub full_path: String,
    //TODO add type enum to get fns by type (use MacroTypes)
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

    // Reads a file and returns a Vector of Strings representing functions that are TuonoMacros or Router generating functions
    pub fn read_module_methods_from_file(
        path: &str,
    ) -> (
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
        Arc<Mutex<Vec<DebugItemFn>>>,
    ) {
        // Only process files with ".rs" extension
        if !path.ends_with(".rs") {
            return (
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
            );
        }

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
                // Only process public functions
                let is_pub = matches!(func.vis, syn::Visibility::Public(_));
                if !is_pub {
                    continue;
                }
                let Ok(debug_fn) = DebugItemFn::try_from(func) else {
                    continue;
                };
                match debug_fn.type_of_fn {
                    TuonoFunction::ApiHandler => api_handlers.push(debug_fn),
                    TuonoFunction::Middleware => middlewares.push(debug_fn),
                    TuonoFunction::Handler => handlers.push(debug_fn),
                    TuonoFunction::RouterGenertor => router_fns.push(debug_fn),
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
                type_of_fn: TuonoFunction::Middleware,
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
                type_of_fn: TuonoFunction::Middleware,
            }]
        );
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
                type_of_fn: TuonoFunction::Middleware,
            }]
        );
        assert_eq!(routers.len(), 0);
    }
}
