use crate::builder::fs::{CreateDirectory, FileWithContents};
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use std::ffi::OsString;
use std::fmt::{Debug, Display};
use std::path::Component;
use std::path::Path as StdPath;
use std::path::PathBuf;

use super::fs::Chmod;
use super::fs::Chown;
use super::fs::ConfigFileData;
use super::users::Group;
use super::users::User;
use super::AsParam;
use super::Context;
use super::PackageInfo;

/// A path into the packages directory. To use files from this directory, expose them with [`PackageContext::expose`]
#[derive(Clone)]
pub struct Source<'a>(pub(crate) &'a PackageInfo);

/// A path that has been exposed with [`PackageContext::expose`].
/// We keep track of the path of the unversioned base path that we exposed.
#[derive(Clone)]
pub struct Exposed(pub(crate) PathBuf);

/// A path inside a created chroot directory.
#[derive(Clone)]
pub struct Chroot;

/// A path that becomes valid once we enter a chroot.
/// We keep track of the base path of the chroot. The real path on the server is constructed by joining `self.0` followed by the path.
#[derive(Clone)]
pub struct Mounted(pub(crate) PathBuf);

/// A file that we will create by copying it from our cache to an arbitrary path.
#[derive(Clone)]
pub struct WillBeCreated;

/// A path to a shared configuration file.
#[derive(Clone)]
pub struct SharedConfig;

/// A path to userdata.
#[derive(Clone)]
pub struct Userdata;

/// A path that should exist on any server with a base ubuntu server install. (like /etc)
#[derive(Clone)]
pub struct Existing;

/// A path that will be created by a package install
#[derive(Clone)]
pub struct FromPackage;

/// A path in the backup folder
#[derive(Clone)]
pub struct Backup;

pub trait SpeculatePath {}
impl SpeculatePath for Exposed {}
impl SpeculatePath for Chroot {}
impl SpeculatePath for WillBeCreated {}
impl SpeculatePath for Existing {}
impl SpeculatePath for Mounted {}

pub trait CanWritePath {}
impl CanWritePath for WillBeCreated {}
impl CanWritePath for SharedConfig {}
impl CanWritePath for Existing {}

#[derive(Clone)]
pub struct Path<L: Clone> {
    pub(crate) base: PathBuf,
    pub(crate) path: PathBuf,
    pub(crate) loc: L,

    // TODO: Move this into the location; Exposed, Source and Existing never need it, the rest always needs it.
    pub(crate) node: Option<GraphNodeReference>,
}

pub fn normalize_path(path: &StdPath) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            // Prefix can only occur at the start, so replacing the return value won't cause any problems
            c @ Component::Prefix(..) => result = <PathBuf as From<_>>::from(c.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    // For relative paths, we need to keep '..' if we don't have any parents to pop
                    result.push(Component::ParentDir);
                }
            }
            Component::Normal(c) => result.push(c),
        }
    }

    result
}

pub fn path_is_safe(path: &StdPath) -> bool {
    let mut num_components = 0;
    for component in path.components() {
        match component {
            // Prefix can only occur at the start, so replacing the return value won't cause any problems
            Component::Prefix(..) => return false,
            Component::RootDir => return false,
            Component::CurDir => (),
            Component::ParentDir => {
                if num_components > 0 {
                    num_components -= 1;
                } else {
                    return false;
                }
            }
            Component::Normal(_) => num_components += 1,
        }
    }

    true
}

impl<L: Clone> Path<L> {
    pub fn graph_node(&self) -> Option<GraphNodeReference> {
        self.node
    }

    pub fn parent(&self) -> Option<Path<L>> {
        self.path.parent().map(|path| Path {
            base: self.base.clone(),
            path: path.to_path_buf(),
            loc: self.loc.clone(),
            node: self.node,
        })
    }

    pub fn file_name(&self) -> Option<OsString> {
        self.full_path().file_name().map(ToOwned::to_owned)
    }

    pub fn full_path(&self) -> PathBuf {
        if !path_is_safe(&self.path) {
            panic!("Path traversal for: {:?}", self.path);
        }

        if self.path.as_os_str().is_empty() {
            self.base.clone()
        } else {
            self.base.join(&self.path)
        }
    }

    pub fn join_unchecked<P: AsRef<StdPath>>(&self, path: P) -> Result<Self, ()> {
        let path = normalize_path(path.as_ref());
        if path.is_absolute() {
            return Err(());
        }

        Ok(Path {
            base: self.base.clone(),
            path: self.path.join(path),
            loc: self.loc.clone(),
            node: self.node,
        })
    }

    pub fn cast_unchecked<T: Clone>(self, loc: T) -> Path<T> {
        Path {
            base: self.base,
            path: self.path,
            loc,
            node: self.node,
        }
    }

    pub fn with_node(self, node: GraphNodeReference) -> Self {
        Path {
            base: self.base,
            path: self.path,
            loc: self.loc,
            node: Some(node),
        }
    }

    pub fn chown<R: Requirement + Supports<Chown>>(
        &self,
        context: &mut Context<R>,
        user: &User,
        group: &Group,
    ) -> GraphNodeReference {
        context.add_node(
            Chown::new(self.full_path(), user.as_param(), group.as_param()),
            self.node.iter(),
        )
    }

    pub fn chmod<R: Requirement + Supports<Chmod>>(
        &self,
        context: &mut Context<R>,
        permissions: u32,
    ) -> GraphNodeReference {
        context.add_node(Chmod::new(self.full_path(), permissions), self.node.iter())
    }
}

impl<L: Clone + SpeculatePath> Path<L> {
    pub fn join<P: AsRef<StdPath>>(&self, path: P) -> Path<L> {
        self.join_unchecked(path).unwrap()
    }
}

impl Path<Mounted> {
    pub fn root(root: &Path<Chroot>) -> Path<Mounted> {
        Path {
            base: <PathBuf as From<_>>::from("/"),
            path: root.path.clone(),
            loc: Mounted(root.base.clone()),
            node: root.node,
        }
    }
}

impl Debug for Path<Source<'_>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Package[{}]", self.path.display())
    }
}

impl Debug for Path<WillBeCreated> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RealPath[{}]", self.full_path().display())
    }
}

impl<L: Clone> Display for Path<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_path().display())
    }
}

impl<L: Clone> AsParam for Path<L> {
    fn as_param(&self) -> String {
        format!("{}", self.full_path().display())
    }
}

impl<'a> Path<Source<'a>> {
    pub fn join<P: AsRef<StdPath>>(&self, name: P) -> Result<Path<Source<'a>>, ()> {
        let new = self.join_unchecked(name)?;
        if self.loc.0.files.contains(&new.full_path()) {
            Ok(new)
        } else {
            panic!("Tried to access non-existant file in package: {}", new);
        }
    }
}

impl Path<Existing> {
    pub fn make_dir<P: AsRef<StdPath>, R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: P,
    ) -> Path<WillBeCreated>
    where
        R: Supports<CreateDirectory>,
    {
        assert!(
            name.as_ref().components().count() == 1
                && matches!(
                    name.as_ref().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(name).unwrap();
        let node = context
            .graph
            .add(CreateDirectory::new(new.full_path()), self.node.as_ref());

        new.cast_unchecked(WillBeCreated).with_node(node)
    }
}

impl Path<Chroot> {
    pub fn make_dir<P: AsRef<StdPath>, R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: P,
    ) -> Path<Chroot>
    where
        R: Supports<CreateDirectory>,
    {
        assert!(
            name.as_ref().components().count() == 1
                && matches!(
                    name.as_ref().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(name).unwrap();
        let node = context.graph.add(
            CreateDirectory::new_without_cleanup(new.full_path()),
            self.node.as_ref(),
        );

        new.with_node(node)
    }

    pub fn rebase_on<L: Clone>(&self, root: &Path<L>) -> Path<Mounted> {
        let root_path = root.full_path();
        let path = self.full_path();
        let path = path
            .strip_prefix(&root_path)
            .expect("You provided a path that is not located in the root");
        Path {
            base: <PathBuf as From<_>>::from("/").join(path),
            path: PathBuf::new(),
            loc: Mounted(root_path),
            node: root.node.or(self.node),
        }
    }
}

impl Path<Userdata> {
    pub fn make_dir<P: AsRef<StdPath>, R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: P,
    ) -> Path<Userdata>
    where
        R: Supports<CreateDirectory>,
    {
        assert!(
            name.as_ref().components().count() == 1
                && matches!(
                    name.as_ref().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(name).unwrap();
        let node = context.graph.add(
            CreateDirectory::new_without_cleanup(new.full_path()),
            self.node.as_ref(),
        );

        new.with_node(node)
    }
}

impl Path<Backup> {
    pub fn make_dir<P: AsRef<StdPath>, R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: P,
    ) -> Path<Backup>
    where
        R: Supports<CreateDirectory>,
    {
        assert!(
            name.as_ref().components().count() == 1
                && matches!(
                    name.as_ref().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(name).unwrap();
        let node = context.graph.add(
            CreateDirectory::new_without_cleanup(new.full_path()),
            self.node.as_ref(),
        );

        new.with_node(node)
    }
}

impl Path<SharedConfig> {
    pub fn make_dir<P: AsRef<StdPath>, R: Requirement>(
        &self,
        context: &mut Context<R>,
        name: P,
    ) -> Path<SharedConfig>
    where
        R: Supports<CreateDirectory>,
    {
        assert!(
            name.as_ref().components().count() == 1
                && matches!(
                    name.as_ref().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(name).unwrap();
        let node = context
            .graph
            .add(CreateDirectory::new(new.full_path()), self.node.as_ref());

        new.with_node(node)
    }

    pub fn make_file<R: Requirement>(
        &self,
        context: &mut Context<R>,
        file: ConfigFileData,
    ) -> Path<SharedConfig>
    where
        R: Supports<FileWithContents>,
    {
        assert!(
            file.path().components().count() == 1
                && matches!(
                    file.path().components().next().unwrap(),
                    Component::Normal(_)
                )
        );

        // unwrap is OK because name is a single, normal, component
        let new = self.join_unchecked(&file.path()).unwrap();
        file.set_full_path(&new)
            .create(context)
            .cast_unchecked(SharedConfig)
    }
}

pub struct BindPath {
    mount_path: PathBuf,
    path_postfix: PathBuf,
    in_dir: Option<Path<Mounted>>,
    nodes: Vec<GraphNodeReference>,
}

pub trait Bindable {
    fn bind(&self) -> BindPath;
}

impl Bindable for Path<Exposed> {
    fn bind(&self) -> BindPath {
        BindPath {
            mount_path: self.loc.0.clone(),
            path_postfix: self
                .full_path()
                .strip_prefix(&self.loc.0)
                .unwrap()
                .to_path_buf(),
            in_dir: None,
            nodes: self.node.iter().copied().collect(),
        }
    }
}

impl Bindable for Path<SharedConfig> {
    fn bind(&self) -> BindPath {
        BindPath {
            mount_path: self.full_path(),
            path_postfix: PathBuf::new(),
            in_dir: None,
            nodes: self.node.iter().copied().collect(),
        }
    }
}

impl Bindable for Path<Backup> {
    fn bind(&self) -> BindPath {
        BindPath {
            mount_path: self.full_path(),
            path_postfix: PathBuf::new(),
            in_dir: None,
            nodes: self.node.iter().copied().collect(),
        }
    }
}

impl Bindable for Path<Existing> {
    fn bind(&self) -> BindPath {
        BindPath {
            mount_path: self.full_path(),
            path_postfix: PathBuf::new(),
            in_dir: None,
            nodes: self.node.iter().copied().collect(),
        }
    }
}

impl Bindable for Path<FromPackage> {
    fn bind(&self) -> BindPath {
        BindPath {
            mount_path: self.full_path(),
            path_postfix: PathBuf::new(),
            in_dir: None,
            nodes: self.node.iter().copied().collect(),
        }
    }
}

impl BindPath {
    pub fn in_dir(mut self, dir: &Path<Mounted>) -> BindPath {
        self.in_dir = Some(dir.clone());
        if let Some(node) = dir.node {
            self.nodes.push(node);
        }

        self
    }

    pub(crate) fn build(
        self,
        root_dir: &PathBuf,
    ) -> (String, Path<Mounted>, Vec<GraphNodeReference>) {
        let postfix = self.path_postfix;
        let join = |path: PathBuf| {
            if postfix.as_os_str().is_empty() {
                path.to_path_buf()
            } else {
                path.join(postfix)
            }
        };

        match self.in_dir {
            Some(in_dir) => {
                assert_eq!(&in_dir.loc.0, root_dir);

                let local_path = in_dir.full_path();
                (
                    format!("{}:{}", self.mount_path.display(), local_path.display()),
                    Path {
                        base: join(local_path),
                        path: PathBuf::new(),
                        loc: Mounted(root_dir.to_path_buf()),
                        node: None, // TODO: Depends on when service starts
                    },
                    self.nodes,
                )
            }
            None => {
                (
                    format!("{}", self.mount_path.display()),
                    Path {
                        base: join(self.mount_path),
                        path: PathBuf::new(),
                        loc: Mounted(root_dir.to_path_buf()),
                        node: None, // TODO: Depends on when service starts
                    },
                    self.nodes,
                )
            }
        }
    }
}

// TODO: Test if path concatenation works properly (no trailing /)
// TODO: Test if bindpath conversion works properly (no trailing /)
