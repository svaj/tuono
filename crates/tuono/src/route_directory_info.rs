use crate::app::{IGNORE_EXTENSIONS, IGNORE_FILES, ROUTES_FOLDER_PATH};
use crate::module_data::{DEFAULT_ROUTER_STR, ModuleData};
use crate::route::Route;
use std::collections::{HashMap, hash_map::Entry};
use std::fmt::Debug;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct RouteDirectoryInfo {
    #[allow(unused)]
    pub base_path: String,
    pub directories: Vec<RouteDirectoryInfo>,
    pub module_data: Vec<ModuleData>,
    pub routes: HashMap<String, Route>, // TODO match up module_data.routers to route & route options if available.
}

// TODO Refactor route collection and route generation in sourceBuilder to match.
// TODO add fn to generate module imports relative to base module, to expose to app's routes dir info
impl RouteDirectoryInfo {
    pub fn new(path: &Path, recurse: bool, base_path: &Path) -> io::Result<RouteDirectoryInfo> {
        if path.is_dir() {
            let mut directories = Vec::new();
            let mut routes: HashMap<String, Route> = HashMap::new();
            let mut module_data: Vec<ModuleData> = Vec::new();
            for entry in fs::read_dir(path)? {
                let entry_path = entry?.path();

                // recursively search directories
                if entry_path.is_dir() {
                    let sub_dir_info = RouteDirectoryInfo::new(&entry_path, recurse, base_path)?;
                    directories.push(sub_dir_info);
                // handle files
                } else if entry_path.is_file() {
                    let file_module_data = ModuleData::new(
                        &entry_path.to_str().expect("Invalid filepath").to_string(),
                    )
                    .unwrap_or_default();
                    module_data.push(file_module_data);
                    // Generate Routes from file, add to routes
                    if RouteDirectoryInfo::should_collect_route(&entry_path) {
                        routes =
                            RouteDirectoryInfo::collect_route(entry_path.to_path_buf(), routes);
                    }
                }
            }

            let dir_info = RouteDirectoryInfo {
                base_path: base_path.to_string_lossy().to_string(),
                directories,
                routes,
                module_data,
            };

            Ok(dir_info)
        } else {
            // If it's not a directory, return an empty DirectoryInfo (though we don't push for non-dirs)
            Ok(RouteDirectoryInfo {
                base_path: RouteDirectoryInfo::get_base_path()
                    .to_string_lossy()
                    .to_string(),
                directories: Vec::new(),
                routes: HashMap::new(),
                module_data: vec![
                    ModuleData::new(&path.to_string_lossy().to_string()).unwrap_or_default(),
                ],
            })
        }
    }

    pub fn get_base_path() -> PathBuf {
        std::env::current_dir().expect("Failed to read current_dir")
    }

    pub fn has_routers(&self) -> bool {
        self.module_data
            .iter()
            .find(|module| module.has_routers())
            .is_some()
    }
    pub fn has_middlewares(&self) -> bool {
        self.module_data
            .iter()
            .find(|module| module.has_middlewares())
            .is_some()
    }

    pub fn get_middleware_modules(&self) -> Vec<ModuleData> {
        self.module_data
            .iter()
            .filter(|&module| module.has_middlewares())
            .cloned()
            .collect()
    }

    pub fn generate_router(&self, ending_semicolon: bool) -> String {
        let semicolon_str = if ending_semicolon { ";" } else { "" };
        if !self.has_routers() {
            return format!("{DEFAULT_ROUTER_STR}{semicolon_str}");
        }

        let modules_with_routers: Vec<&ModuleData> = self
            .module_data
            .iter()
            .filter(|&m| m.has_routers())
            .collect();

        let full_paths: Vec<String> = modules_with_routers
            .clone()
            .into_iter()
            .map(|m| m.full_path.to_string())
            .collect();
        if full_paths.len() as i32 > 1 {
            println!(
                "Warning, Route directory with more than 1 module with router functions found ({}), using first ({})",
                full_paths.join(", "),
                full_paths[0]
            )
        }
        // just use the first router function we find in this dir.
        modules_with_routers[0].generate_router(ending_semicolon)
    }

    pub fn should_collect_route(entry: &Path) -> bool {
        let file_extension = entry.extension().expect("Failed to read file extension");
        let file_name = entry.file_stem().expect("Failed to read file name");

        if IGNORE_EXTENSIONS.iter().any(|val| val == &file_extension) {
            return false;
        }

        if IGNORE_FILES.iter().any(|val| val == &file_name) {
            return false;
        }
        true
    }

    fn collect_route(entry: PathBuf, routes: HashMap<String, Route>) -> HashMap<String, Route> {
        let mut ret_routes: HashMap<String, Route> = routes.clone();
        let base_path = RouteDirectoryInfo::get_base_path();
        let base_path_str = base_path.to_string_lossy();
        let path = entry
            .to_str()
            .expect("Failed to read entry as str")
            .replace(&format!("{base_path_str}{ROUTES_FOLDER_PATH}"), "")
            // Cleanup windows paths
            .replace("\\", "/")
            .replace(".rs", "")
            .replace(".mdx", "")
            .replace(".tsx", "");

        if entry.extension().expect("failed to read entry extension") == "rs" {
            if let Entry::Vacant(routes) = ret_routes.entry(path.clone()) {
                let mut route = Route::new(path);
                route.update_axum_info();
                routes.insert(route);
            } else {
                let route = ret_routes.get_mut(&path).unwrap();
                route.update_axum_info();
            }
            return ret_routes;
        }
        if let Entry::Vacant(routes) = ret_routes.entry(path.clone()) {
            let route = Route::new(path);
            routes.insert(route);
        }
        ret_routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_get_base_path() {
        let path = RouteDirectoryInfo::get_base_path();
        assert!(path.is_absolute());
    }

    #[test]
    fn test_should_collect_route() {
        let temp_dir = TempDir::new().unwrap();
        let rs_file = temp_dir.path().join("test.rs");
        File::create(&rs_file).unwrap();
        assert!(RouteDirectoryInfo::should_collect_route(&rs_file));

        let tsx_file = temp_dir.path().join("test.tsx");
        File::create(&tsx_file).unwrap();
        assert!(RouteDirectoryInfo::should_collect_route(&tsx_file));

        let md_file = temp_dir.path().join("test.md");
        File::create(&md_file).unwrap();
        assert!(RouteDirectoryInfo::should_collect_route(&md_file));
    }

    #[test]
    fn test_collect_route() {
        let temp_dir = TempDir::new().unwrap();
        let rs_file = temp_dir.path().join("index.rs");
        File::create(&rs_file).unwrap();

        let routes = HashMap::new();
        let new_routes = RouteDirectoryInfo::collect_route(rs_file, routes);
        assert!(!new_routes.is_empty());
    }

    #[test]
    fn test_route_directory_info_new() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("sub");
        std::fs::create_dir(&sub_dir).unwrap();
        let file = temp_dir.path().join("test.rs");
        File::create(&file).unwrap();
        let middlewares_file = temp_dir.path().join("middlewares.rs");
        let mut file = File::create(&middlewares_file).unwrap();
        writeln!(file, "#[tuono_lib::middleware]\nfn test_middleware() {{}}").unwrap();

        let dir_info = RouteDirectoryInfo::new(&temp_dir.path(), true, &temp_dir.path()).unwrap();
        assert!(!dir_info.directories.is_empty());
        assert!(!dir_info.module_data.is_empty());
        assert!(!dir_info.get_middleware_modules().is_empty());
    }
}
