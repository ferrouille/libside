use crate::{generic_apt_package, graph::GraphNodeReference};
use std::path::PathBuf;

use super::{
    apt::AptPackage,
    path::{FromPackage, Path},
    systemd::SystemdService,
};

pub trait PhpVersion {
    const APT_PHP_FPM: &'static str;
    const BINARY: &'static str;
    const SERVICE: &'static str;

    fn new() -> Self;
}

pub struct Php74;

impl PhpVersion for Php74 {
    const APT_PHP_FPM: &'static str = "php7.4-fpm";
    const BINARY: &'static str = "/usr/sbin/php-fpm7.4";
    const SERVICE: &'static str = "php7.4-fpm";

    fn new() -> Self {
        Php74
    }
}

pub struct Php80;

impl PhpVersion for Php80 {
    const APT_PHP_FPM: &'static str = "php8.0-fpm";
    const BINARY: &'static str = "/usr/sbin/php-fpm8.0";
    const SERVICE: &'static str = "php8.0-fpm";

    fn new() -> Self {
        Php80
    }
}

pub struct PhpFpm<V>(GraphNodeReference, V);

impl<V: PhpVersion> AptPackage for PhpFpm<V> {
    const NAME: &'static str = V::APT_PHP_FPM;

    fn create(node: GraphNodeReference) -> Self {
        PhpFpm(node, V::new())
    }

    fn graph_node(&self) -> GraphNodeReference {
        self.0
    }
}

impl<V: PhpVersion> PhpFpm<V> {
    pub fn binary(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from(V::BINARY),
            path: PathBuf::new(),
            loc: FromPackage,
            node: Some(self.graph_node()),
        }
    }

    pub fn default_service(&self) -> SystemdService {
        SystemdService::from_name_unchecked(V::SERVICE, self.graph_node(), vec![self.graph_node()])
    }
}

generic_apt_package!(pub PhpMySql => "php-mysql");
