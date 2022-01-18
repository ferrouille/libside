use std::path::PathBuf;

use super::{
    apt::AptPackage,
    path::{FromPackage, Path},
};

impl AptPackage<"php8.0-fpm"> {
    pub fn binary(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from("/usr/sbin/php-fpm8.0"),
            path: PathBuf::new(),
            loc: FromPackage,
            node: Some(self.graph_node()),
        }
    }

    pub fn default_service_files(&self) -> Vec<Path<FromPackage>> {
        vec![
            Path {
                base: PathBuf::from("/lib/systemd/system/php8.0-fpm.service"),
                path: PathBuf::new(),
                loc: FromPackage,
                node: Some(self.graph_node()),
            },
            Path {
                base: PathBuf::from("/etc/init.d/php8.0-fpm"),
                path: PathBuf::new(),
                loc: FromPackage,
                node: Some(self.graph_node()),
            },
        ]
    }
}

impl AptPackage<"php7.4-fpm"> {
    pub fn binary(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from("/usr/sbin/php-fpm7.4"),
            path: PathBuf::new(),
            loc: FromPackage,
            node: Some(self.graph_node()),
        }
    }

    pub fn default_service_files(&self) -> Vec<Path<FromPackage>> {
        vec![
            Path {
                base: PathBuf::from("/lib/systemd/system/php7.4-fpm.service"),
                path: PathBuf::new(),
                loc: FromPackage,
                node: Some(self.graph_node()),
            },
            Path {
                base: PathBuf::from("/etc/init.d/php7.4-fpm"),
                path: PathBuf::new(),
                loc: FromPackage,
                node: Some(self.graph_node()),
            },
        ]
    }
}
