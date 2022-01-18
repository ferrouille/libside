use std::path::PathBuf;

use super::{
    path::{FromPackage, Path},
    Group, User,
    {apt::AptPackage, systemd::SystemdService},
};

impl AptPackage<"nginx"> {
    pub fn binary(&self) -> Path<FromPackage> {
        Path {
            base: PathBuf::from("/usr/sbin/nginx"),
            path: PathBuf::new(),
            loc: FromPackage,
            node: Some(self.graph_node()),
        }
    }

    pub fn default_service(&self) -> SystemdService {
        SystemdService::new("nginx", self.graph_node(), vec![self.graph_node()])
    }

    pub fn www_data_user(&self) -> User {
        User {
            name: "www-data".to_owned(),
            node: self.graph_node(),
        }
    }

    pub fn www_data_group(&self) -> Group {
        Group {
            name: "www-data".to_owned(),
            node: self.graph_node(),
        }
    }
}

// pub struct SiteConfig<'a> {
//     writer: &'a mut String,
//     indent: usize,
// }

// impl SiteConfig<'_> {
//     pub fn create<T>(f: impl Fn(&mut SiteConfig<'_>), out: impl Fn(FileData<'_>) -> T) -> T {
//         let mut writer = String::new();
//         let mut node = SiteConfig {
//             writer: &mut writer,
//             indent: 0,
//         };

//         f(&mut node);

//         out(FileData::new("[site configuration]", &writer))
//     }

//     pub fn block<N: AsRef<str>>(&mut self, name: N, f: impl Fn(&mut SiteConfig<'_>)) -> &mut Self {
//         writeln!(self.writer, "{}{} {{", " ".repeat(self.indent), name.as_ref()).unwrap();

//         f(&mut SiteConfig {
//             writer: self.writer,
//             indent: self.indent + 4,
//         });

//         writeln!(self.writer, "{}}}", " ".repeat(self.indent)).unwrap();

//         self
//     }

//     pub fn set<N: AsRef<str>, V: AsRef<str>>(&mut self, name: N, value: V) -> &mut Self {
//         writeln!(self.writer, "{}{} {};", " ".repeat(self.indent), name.as_ref(), value.as_ref()).unwrap();

//         self
//     }

//     pub fn server(&mut self, f: impl Fn(&mut SiteConfig<'_>)) -> &mut Self { self.block("server", f) }
//     pub fn directory<T: Tag, N: AsRef<str>>(&mut self, name: N, directory: &ExistingDirectory<T>) -> &mut Self {
//         self.set(name, directory.path().to_str().unwrap())
//     }

//     pub fn end(&mut self) {}
// }