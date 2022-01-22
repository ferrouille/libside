use self::apply::PreparedBuild;
use self::fs::{CreateDirectory, Delete};
use self::users::{Group, User};
use crate::requirements::{Requirement, Supports};
use crate::{
    graph::{Graph, GraphNodeReference, Pending},
    secrets::{Secret, SecretId, Secrets},
    Dirs, StateDirs, VersionedPath,
};
use path::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path as StdPath;
use std::{
    fmt::{Debug, Display},
    io,
    path::PathBuf,
};
use typemap::{Key, TypeMap};

pub mod apply;
pub mod apt;
pub mod fs;
pub mod mysql;
pub mod nginx;
pub mod path;
pub mod php_fpm;
pub mod systemd;
pub mod users;

#[derive(Serialize, Deserialize)]
pub struct PackageConfig<C> {
    #[serde(flatten)]
    config: C,
}

pub struct PackageInfo {
    name: String,
    path: PathBuf,
    files: Vec<PathBuf>,
}

pub struct Package<C> {
    info: PackageInfo,
    config: PackageConfig<C>,
}

impl<C> Package<C> {
    pub fn name(&self) -> &str {
        self.info.name.as_str()
    }

    pub fn config(&self) -> &C {
        &self.config.config
    }

    pub fn root<'a>(&'a self) -> Path<Source<'a>> {
        Path {
            base: self.info.path.clone(),
            path: PathBuf::new(),
            loc: Source(&self.info),
            node: None,
        }
    }
}

struct ExposedPath {
    source: PathBuf,
    target: PathBuf,
}

pub trait Builder {
    type PackageConfig: DeserializeOwned;
    type Data;
    type Requirement: Requirement + Display;
    type BuildError: std::error::Error;

    fn start_build(
        &self,
        context: &mut Context<Self::Requirement>,
    ) -> Result<Self::Data, Self::BuildError>;
    fn build_package(
        &self,
        package: &Package<Self::PackageConfig>,
        context: &mut Context<Self::Requirement>,
        data: &mut Self::Data,
    ) -> Result<(), Self::BuildError>;
    fn finish_build(
        &self,
        context: &mut Context<Self::Requirement>,
        data: Self::Data,
    ) -> Result<(), Self::BuildError>;
}

pub struct GeneratedFile {
    source: PathBuf,
    contents: Vec<u8>,
    needs_cleanup: bool,
}

#[derive(Debug)]
pub struct DeletedFile {
    save_to: PathBuf,
}

#[derive(Debug)]
pub struct ConfigDir {
    target: PathBuf,
    needs_cleanup: bool,
}

pub trait AsParam {
    fn as_param(&self) -> String;
}

impl<T: AsParam> AsParam for &T {
    fn as_param(&self) -> String {
        T::as_param(*self)
    }
}

impl AsParam for String {
    fn as_param(&self) -> String {
        self.clone()
    }
}

impl AsParam for &str {
    fn as_param(&self) -> String {
        self.to_string()
    }
}

pub struct Context<'a, R> {
    source_root: Path<Source<'a>>,
    files: Vec<GeneratedFile>,
    deleted_files: Vec<DeletedFile>,
    exposed: Vec<ExposedPath>,
    package_name: String,

    info: &'a PackageInfo,
    install: &'a StateDirs,
    secrets: &'a mut Secrets,

    graph: &'a mut Graph<R, Pending>,

    generated_path: PathBuf,
    chroots_path: Option<Path<Chroot>>,
    config_files_path: Option<Path<SharedConfig>>,
    exposed_files_path: VersionedPath,
    userdata_path: Option<Path<Userdata>>,
    backup_path: Option<Path<Backup>>,
    deleted_path: PathBuf,

    state: &'a mut TypeMap,
}

pub struct MinimalContext {
    files: Vec<GeneratedFile>,
    deleted_files: Vec<DeletedFile>,
    exposed: Vec<ExposedPath>,
}

impl<'a, R: Requirement> Context<'a, R> {
    pub fn new(
        info: &'a PackageInfo,
        dirs: &Dirs,
        install: &'a StateDirs,
        secrets: &'a mut Secrets,
        graph: &'a mut Graph<R, Pending>,
        state: &'a mut TypeMap,
    ) -> Self {
        let p = Context {
            info,
            install,
            source_root: Path::<Source> {
                base: info.path.to_owned(),
                path: PathBuf::new(),
                loc: Source(info),
                node: None,
            },
            secrets,
            graph,
            files: Vec::new(),
            deleted_files: Vec::new(),
            exposed: Default::default(),
            package_name: info.name.to_string(),
            generated_path: install.generated_path(&info.name),
            chroots_path: None,
            config_files_path: None,
            exposed_files_path: install.exposed_path(&info.name),
            userdata_path: None,
            backup_path: None,
            deleted_path: dirs.deleted.clone(),
            state,
        };

        p
    }

    pub fn add_node<'r, N, I: IntoIterator<Item = &'r GraphNodeReference>>(
        &mut self,
        node: N,
        deps: I,
    ) -> GraphNodeReference
    where
        R: Supports<N>,
    {
        self.graph.add(node, deps)
    }

    pub fn into_minimal(self) -> MinimalContext {
        MinimalContext {
            files: self.files,
            deleted_files: self.deleted_files,
            exposed: self.exposed,
        }
    }

    pub fn package_root(&self) -> Path<Source<'a>> {
        self.source_root.clone()
    }

    pub fn expose(&mut self, path: &Path<Source>) -> Path<Exposed> {
        let full_path = path.full_path();
        if !full_path.exists() {
            panic!("Exposed path does not exist: {:?}", full_path);
        }

        let name = full_path.file_name().unwrap();
        let mut target = self.exposed_files_path.join(name);
        let mut k = 0u64;

        loop {
            if self.exposed.iter().any(|e| e.target == target.full_path()) {
                let mut name = full_path.file_stem().unwrap().to_owned();
                name.push(k.to_string());
                name.push(full_path.extension().unwrap());
                target = self.exposed_files_path.join(name);
                k += 1;
            } else {
                break;
            }
        }

        self.exposed.push(ExposedPath {
            source: full_path,
            target: target.full_path(),
        });

        Path {
            base: target.full_path(),
            path: PathBuf::new(),
            loc: Exposed(target.unversioned_path().to_path_buf()),
            node: None,
        }
    }

    pub fn create_chroot<P: AsRef<StdPath>>(&mut self, name: P) -> Path<Chroot>
    where
        R: Supports<CreateDirectory>,
    {
        let name = name.as_ref();
        assert!(path_is_safe(name));

        let chroots_root = self.chroots_root();
        let path = chroots_root.join(name).full_path();
        let node = self.graph.add(
            CreateDirectory::new_without_cleanup(path.clone()),
            chroots_root.node.as_ref(),
        );

        Path {
            base: path,
            path: PathBuf::new(),
            loc: Chroot,
            node: Some(node),
        }
    }

    pub fn create_userdata<P: AsRef<StdPath>>(&mut self, name: P) -> Path<Userdata>
    where
        R: Supports<CreateDirectory>,
    {
        let name = name.as_ref();
        assert!(path_is_safe(name));

        let userdata_root = self.userdata_root();
        let path = userdata_root.join_unchecked(name).unwrap().full_path();
        let node = self.graph.add(
            CreateDirectory::new_without_cleanup(path.clone()),
            userdata_root.node.as_ref(),
        );

        Path {
            base: path,
            path: PathBuf::new(),
            loc: Userdata,
            node: Some(node),
        }
    }

    fn chroots_root(&mut self) -> Path<Chroot>
    where
        R: Supports<CreateDirectory>,
    {
        let install = self.install;
        let info = self.info;
        let graph = &mut self.graph;
        self.chroots_path
            .get_or_insert_with(|| {
                let chroots_path = install.chroot_path(&info.name);
                let chroots_ref = graph.add(
                    CreateDirectory::new_without_cleanup(chroots_path.clone()),
                    [],
                );
                Path {
                    base: chroots_path,
                    path: PathBuf::new(),
                    loc: Chroot,
                    node: Some(chroots_ref),
                }
            })
            .clone()
    }

    fn userdata_root(&mut self) -> Path<Userdata>
    where
        R: Supports<CreateDirectory>,
    {
        let install = self.install;
        let info = self.info;
        let graph = &mut self.graph;
        self.userdata_path
            .get_or_insert_with(|| {
                let userdata_path = install.userdata_path(&info.name);
                let parent_ref = graph.add(
                    CreateDirectory::new_without_cleanup(
                        userdata_path.clone().parent().unwrap().to_path_buf(),
                    ),
                    [],
                );
                let userdata_ref = graph.add(
                    CreateDirectory::new_without_cleanup(userdata_path.clone()),
                    &[parent_ref],
                );
                Path {
                    base: userdata_path,
                    path: PathBuf::new(),
                    loc: Userdata,
                    node: Some(userdata_ref),
                }
            })
            .clone()
    }

    pub fn config_root(&mut self) -> Path<SharedConfig>
    where
        R: Supports<CreateDirectory>,
    {
        let install = self.install;
        let info = self.info;
        let graph = &mut self.graph;
        self.config_files_path
            .get_or_insert_with(|| {
                let config_files_path = install.config_path(&info.name);
                let config_files_ref = graph.add(
                    CreateDirectory::new_without_cleanup(config_files_path.clone()),
                    &[],
                );
                Path {
                    base: config_files_path,
                    path: PathBuf::new(),
                    loc: SharedConfig,
                    node: Some(config_files_ref),
                }
            })
            .clone()
    }

    pub fn backup_root(&mut self) -> Path<Backup>
    where
        R: Supports<CreateDirectory>,
    {
        let install = self.install;
        let info = self.info;
        let graph = &mut self.graph;
        self.backup_path
            .get_or_insert_with(|| {
                let backup_path = install.backup_path(&info.name);
                let backup_ref = graph.add(
                    CreateDirectory::new_without_cleanup(backup_path.clone()),
                    &[],
                );
                Path {
                    base: backup_path,
                    path: PathBuf::new(),
                    loc: Backup,
                    node: Some(backup_ref),
                }
            })
            .clone()
    }

    pub fn shared_backup_root(&mut self) -> Path<Backup> {
        let install = self.install;
        Path {
            base: install.backup.clone(),
            path: PathBuf::new(),
            loc: Backup,
            node: None,
        }
    }

    pub fn delete_default_system_file<L: Clone>(&mut self, path: Path<L>) -> GraphNodeReference
    where
        R: Supports<Delete>,
    {
        let full_path = path.full_path();
        assert!(full_path.is_absolute());

        let backup_path = self
            .deleted_path
            .join(&full_path.strip_prefix("/").unwrap());
        self.deleted_files.push(DeletedFile {
            save_to: backup_path.clone(),
        });
        self.graph
            .add(Delete::new(full_path, backup_path), path.node.iter())
    }

    pub fn existing<P: AsRef<StdPath>>(&mut self, path: P) -> Path<Existing> {
        let path = path.as_ref();
        println!("TODO: Verify must_exist {}", path.display());
        // self.must_exist.push(path.to_path_buf());
        Path {
            base: path.to_path_buf(),
            path: PathBuf::new(),
            loc: Existing,
            node: None,
        }
    }

    pub fn secret<T: Secret + std::fmt::Debug>(&mut self, name: &str) -> T {
        self.secrets
            .get_or_create(SecretId::new(self.package_name.clone(), name.to_string()))
            .unwrap()
    }

    pub fn state<T: Default + 'static>(&mut self) -> &mut T {
        self.state
            .entry::<SimpleKv<T>>()
            .or_insert_with(Default::default)
    }
}

struct SimpleKv<T>(T);

impl<T: 'static> Key for SimpleKv<T> {
    type Value = T;
}

fn scan_files(path: &StdPath) -> Result<Vec<PathBuf>, io::Error> {
    let mut result = Vec::new();

    let mut stack = Vec::new();
    stack.push(path.to_path_buf());

    while let Some(working_path) = stack.pop() {
        for entry in std::fs::read_dir(&working_path)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                stack.push(path.clone());
            }

            result.push(path);
        }
    }

    Ok(result)
}

pub struct Packages<C> {
    packages: Vec<Package<C>>,
}

impl<C: DeserializeOwned> Packages<C> {
    pub fn load(dirs: &Dirs) -> Result<Packages<C>, io::Error> {
        let package_dir = &dirs.packages;
        let mut packages = Vec::new();
        for package_path in package_dir.read_dir()? {
            let package_path = package_path?;
            let file_type = package_path.file_type()?;
            if file_type.is_dir() {
                let path = package_path.path();
                let config =
                    toml::from_str(std::fs::read_to_string(path.join("package.toml"))?.as_str())
                        .unwrap();
                let name = path.file_name().unwrap().to_string_lossy().to_string();

                if name == "_start" || name == "_finish" {
                    panic!("Invalid package name: {}", name);
                }

                let info = PackageInfo {
                    name,
                    files: scan_files(&path)?,
                    path,
                };

                packages.push(Package { info, config });
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, String::new()));
            }
        }

        Ok(Packages { packages })
    }
}

pub fn run<'d, K, B: Builder<PackageConfig = K>>(
    dirs: &Dirs,
    packages: Packages<K>,
    install: &'d StateDirs,
    builder: B,
) -> Result<PreparedBuild<'d, B::Requirement>, B::BuildError>
where
    B::Requirement: Supports<CreateDirectory>,
{
    let packages = packages.packages;
    let mut graph = Graph::new();
    let mut contexts = Vec::new();

    let mut secrets = Secrets::load(&dirs.secrets).unwrap();

    let start = PackageInfo {
        name: String::from("_start"),
        path: PathBuf::new(),
        files: Vec::new(),
    };

    let mut state = TypeMap::new();

    println!("Preparing global..");
    let mut context = Context::new(
        &start,
        &dirs,
        &install,
        &mut secrets,
        &mut graph,
        &mut state,
    );
    let mut data = builder.start_build(&mut context)?;
    contexts.push(context.into_minimal());

    for package in packages.iter() {
        println!("Preparing package {}..", package.info.name);
        let mut context = Context::new(
            &package.info,
            &dirs,
            &install,
            &mut secrets,
            &mut graph,
            &mut state,
        );
        builder.build_package(&package, &mut context, &mut data)?;

        contexts.push(context.into_minimal());
    }

    let finish = PackageInfo {
        name: String::from("_finish"),
        path: PathBuf::new(),
        files: Vec::new(),
    };
    let mut context = Context::new(
        &finish,
        &dirs,
        &install,
        &mut secrets,
        &mut graph,
        &mut state,
    );
    builder.finish_build(&mut context, data)?;
    contexts.push(context.into_minimal());

    secrets.save(&dirs.secrets).unwrap();

    Ok(PreparedBuild::new(install, contexts, graph))
}
