use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::path::PathBuf;

use clap::crate_version;
use tracing::error;

use crate::app::App;
use crate::mode::Mode;
use crate::module_data::ModuleData;
use crate::route::AxumInfo;
use crate::route::Route;
use crate::route_directory_info::RouteDirectoryInfo;
use crate::typescript::TypesJar;

#[cfg(not(target_os = "windows"))]
const FALLBACK_HTML: &str = include_str!("../templates/fallback.html");
#[cfg(not(target_os = "windows"))]
const SERVER_ENTRY_DATA: &str = include_str!("../templates/server.ts");
#[cfg(not(target_os = "windows"))]
const CLIENT_ENTRY_DATA: &str = include_str!("../templates/client.ts");
#[cfg(not(target_os = "windows"))]
const AXUM_ENTRY_POINT: &str = include_str!("../templates/server.rs");

#[cfg(not(target_os = "windows"))]
const MAIN_FILE_PATH: &str = "./.tuono/main.rs";

#[cfg(not(target_os = "windows"))]
const FALLBACK_HTML_PATH: &str = "./.tuono/index.html";

const ROUTE_FOLDER: &str = "src/routes";
const DEV_FOLDER: &str = ".tuono";

#[cfg(target_os = "windows")]
const FALLBACK_HTML: &str = include_str!("..\\templates\\fallback.html");
#[cfg(target_os = "windows")]
const SERVER_ENTRY_DATA: &str = include_str!("..\\templates\\server.ts");
#[cfg(target_os = "windows")]
const CLIENT_ENTRY_DATA: &str = include_str!("..\\templates\\client.ts");
#[cfg(target_os = "windows")]
const AXUM_ENTRY_POINT: &str = include_str!("..\\templates\\server.rs");

#[cfg(target_os = "windows")]
const MAIN_FILE_PATH: &str = ".\\.tuono\\main.rs";

#[cfg(target_os = "windows")]
const FALLBACK_HTML_PATH: &str = ".\\.tuono\\index.html";

// Use this function to instruct the users on how to
// fix their setup to make tuono work
fn recoverable_error(message: &str) -> ! {
    error!("{}", message);
    std::process::exit(1);
}

// Struct to build the source code
// on both "dev" and "build" commands
#[derive(Clone, Debug)]
pub struct SourceBuilder {
    pub app: App,
    mode: Mode,
    base_path: PathBuf,
    types_jar: TypesJar,
}

impl SourceBuilder {
    pub fn new(mode: Mode) -> io::Result<Self> {
        if !PathBuf::from("tuono.config.ts").exists() {
            recoverable_error("Cannot find tuono.config.ts - is this a tuono project?");
        }

        let dev_folder = Path::new(DEV_FOLDER);
        if !&dev_folder.is_dir() {
            fs::create_dir(dev_folder)?;
        }

        let app = App::new();

        let base_path = std::env::current_dir()?;

        Ok(Self {
            app,
            mode,
            types_jar: TypesJar::from(&base_path),
            base_path,
        })
    }

    // Build the source code needed for both build and dev
    pub fn base_build(&mut self) -> io::Result<()> {
        let mode = self.mode.clone();

        self.refresh_axum_source()?;
        let dev_folder = Path::new(DEV_FOLDER);
        self.create_file(dev_folder.join("server-main.tsx"), SERVER_ENTRY_DATA)?;
        self.create_file(dev_folder.join("client-main.tsx"), CLIENT_ENTRY_DATA)?;

        self.types_jar.generate_typescript_file(&self.base_path)?;

        if mode == Mode::Dev {
            self.app.build_tuono_config()?;
            let fallback_html = self.build_html_fallback();
            self.create_file(PathBuf::from(FALLBACK_HTML_PATH), &fallback_html)?;
        }

        Ok(())
    }
    fn generate_axum_source(&self) -> String {
        let Self { app, mode, .. } = &self;
        let app_dir_info = &app.app_directory_info;

        let generated_router = app_dir_info.generate_router(false);
        let mut main_file_definition: &str = " let router = Router::new()";
        let mut main_file_usage = ";".to_string();
        let mut mainfile_import: &str = "";
        let mode_str = mode.as_str();
        if app.has_app_state {
            main_file_definition = "let user_custom_state = tuono_main_state::main().await;\n 
            let router = Router::new()";
            if app_dir_info.has_routers() {
                main_file_usage = format!(".merge(tuono_main_state::{generated_router});");
            }
            mainfile_import = r#"#[path="../src/app.rs"]
            mod tuono_main_state;
            "#;
        }
        let src = AXUM_ENTRY_POINT
            .replace("\r", "")
            .replace(
                "// ROUTE_BUILDER\n",
                &self.create_routes_declaration(&app.route_directory_info),
            )
            .replace(
                "// MODULE_IMPORTS\n",
                &self.create_modules_declaration(&app.route_directory_info),
            )
            .replace("/*VERSION*/", crate_version!())
            .replace(
                "/*MODE*/",
                format!("const MODE: Mode = {mode_str};").as_ref(),
            )
            .replace("//MAIN_FILE_IMPORT//", mainfile_import)
            .replace("//MAIN_FILE_DEFINITION//", main_file_definition)
            .replace("//MAIN_FILE_USAGE//", &main_file_usage);

        let mut import_http_handler = String::new();

        let used_http_methods = app.get_used_http_methods();

        for method in used_http_methods.into_iter() {
            let method = method.to_string().to_lowercase();
            import_http_handler.push_str(&format!("use tuono_lib::axum::routing::{method};\n"))
        }

        src.replace("// AXUM_GET_ROUTE_HANDLER", &import_http_handler)
    }

    pub fn refresh_axum_source(&self) -> io::Result<()> {
        let axum_source = self.generate_axum_source();

        self.create_file(PathBuf::from(MAIN_FILE_PATH), &axum_source)?;

        Ok(())
    }

    fn create_file(&self, path: PathBuf, content: &str) -> io::Result<()> {
        let mut data_file = fs::File::create(self.base_path.join(path))?;

        data_file.write_all(content.as_bytes())?;

        Ok(())
    }

    pub fn refresh_typescript_file(&mut self, path: PathBuf) {
        self.types_jar.refresh_file(path);
    }

    pub fn remove_typescript_file(&mut self, path: PathBuf) {
        self.types_jar.remove_file(path);
    }

    pub fn generate_typescript_file(&mut self) -> io::Result<()> {
        self.types_jar.generate_typescript_file(&self.base_path)
    }

    // Adds calls to .layer() for adding middleware to axum
    pub fn add_route_layers(&self, module: &ModuleData) -> String {
        let mut layers_str = String::from("");

        if module.has_middlewares() {
            let middleware_import = &module.get_module_import();
            let layers = &module.middlewares.lock().unwrap();
            for layer in layers.iter() {
                let middleware_fn_call = layer.fn_call_str.clone();
                layers_str.push_str(&format!(
                    r#".layer({middleware_import}::{middleware_fn_call})
                            "#
                ));
            }
        }
        layers_str
    }

    // TODO generating the import should live in RouteDirectoryInfo and called from here.  Add router generation per directory.
    // Adds Routers with routes to axum
    fn create_routes_declaration(&self, route_directory_info: &RouteDirectoryInfo) -> String {
        let routes = route_directory_info.routes.clone();
        let mut route_declarations = String::from("// ROUTE_BUILDER\n");
        let generated_router = route_directory_info.generate_router(false);
        route_declarations.push_str(format!(r#".merge({generated_router}"#).as_str());
        // Group by directory, find dirs with middleware, have that spit out Router::new() with routes and middlewares

        // directories can have their own middlewares and routes, recurse into that directory to get route/middleware info
        for directory in route_directory_info.directories.clone() {
            route_declarations.push_str(&self.create_routes_declaration(&directory));
        }
        for (_key, route) in routes {
            let Route { axum_info, .. } = &route;
            if !axum_info.is_some() {
                continue;
            }
            // TODO this should be on a route's module data and import should be relative to base module's crate if public
            let AxumInfo {
                axum_route,
                module_import,
            } = axum_info.as_ref().unwrap();
            if !route.is_api() {
                route_declarations.push_str(&format!(
                    r#".route("{axum_route}", get({module_import}::tuono_internal_route))"#
                ));

                route_declarations.push_str(&format!(
                    r#".route("/__tuono/data{axum_route}", get({module_import}::tuono_internal_api))"#
                ));
            } else {
                for method in route.api_data.as_ref().unwrap().methods.clone() {
                    let method = method.to_string().to_lowercase();
                    route_declarations.push_str(&format!(
                            r#".route("{axum_route}", {method}({module_import}::{method}_tuono_internal_api))"#
                    ));
                }
            }
        }

        route_declarations.push_str(")\n");

        // add directory level middleware to routes

        if route_directory_info.has_middlewares() {
            for module in route_directory_info.get_middleware_modules().iter() {
                route_declarations.push_str(&self.add_route_layers(module));
            }
        }

        route_declarations
    }

    fn create_modules_declaration(&self, route_directory_info: &RouteDirectoryInfo) -> String {
        let routes = &route_directory_info.routes;
        let mut module_declarations = String::from("// MODULE_IMPORTS\n");

        // add subdirectory module declarations
        for directory in route_directory_info.directories.clone() {
            module_declarations.push_str(&self.create_modules_declaration(&directory));
        }

        // add module import per route
        for (path, route) in routes.iter() {
            if let Some(route_auxm_info) = &route.axum_info {
                let AxumInfo { module_import, .. } = route_auxm_info;

                module_declarations.push_str(&format!(
                    r#"#[path="../{ROUTE_FOLDER}{path}.rs"]
                    mod {module_import};
                    "#
                ));
            }
        }
        for module in route_directory_info.module_data.iter() {
            if module.has_middlewares() || module.has_routers() {
                module_declarations.push_str(&module.get_pathed_module_use_str())
            }
        }

        module_declarations
    }

    fn build_html_fallback(&self) -> String {
        if let Some(config) = &self.app.config.as_ref() {
            if let Some(origin) = &config.server.origin {
                FALLBACK_HTML.replace("[BASE_URL]", origin)
            } else {
                let url = format!("http://{}:{}", config.server.host, config.server.port);
                FALLBACK_HTML.replace("[BASE_URL]", url.as_str())
            }
        } else {
            "".to_string()
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_set_the_correct_mode() {
        let dev_bundle = SourceBuilder {
            app: App::new(),
            mode: Mode::Dev,
            base_path: PathBuf::new(),
            types_jar: TypesJar::default(),
        }
        .generate_axum_source();

        let prod_bundle = SourceBuilder {
            app: App::new(),
            mode: Mode::Prod,
            base_path: PathBuf::new(),
            types_jar: TypesJar::default(),
        }
        .generate_axum_source();

        assert!(dev_bundle.contains("const MODE: Mode = Mode::Dev;"));
        assert!(prod_bundle.contains("const MODE: Mode = Mode::Prod;"));
    }

    #[test]
    fn should_not_load_the_axum_get_function() {
        let dev_bundle = SourceBuilder {
            app: App::new(),
            mode: Mode::Dev,
            base_path: PathBuf::new(),
            types_jar: TypesJar::default(),
        }
        .generate_axum_source();

        assert!(!dev_bundle.contains("use tuono_lib::axum::routing::get;"));
    }

    #[test]
    fn should_load_the_axum_get_function() {
        let mut source_builder = SourceBuilder {
            app: App::new(),
            mode: Mode::Dev,
            base_path: PathBuf::new(),
            types_jar: TypesJar::default(),
        };

        let mut route = Route::new(String::from("index.tsx"));
        route.update_axum_info();

        source_builder
            .app
            .route_map
            .insert(String::from("index.rs"), route);

        let dev_bundle = source_builder.generate_axum_source();

        assert!(dev_bundle.contains("use tuono_lib::axum::routing::get;"));
    }

    #[test]
    fn should_create_fallback_html_with_default_config() {
        let mut app = App::new();
        app.config = Some(Default::default());

        let source_builder = SourceBuilder {
            app,
            mode: Mode::Dev,
            base_path: PathBuf::new(),
            types_jar: TypesJar::default(),
        };

        let fallback_html = source_builder.build_html_fallback();

        assert!(fallback_html.contains("http://localhost:3000/vite-server/@react-refresh"));
        assert!(fallback_html.contains("http://localhost:3000/vite-server/@vite/client"));
        assert!(fallback_html.contains("http://localhost:3000/vite-server/client-main.tsx"));
    }
}
