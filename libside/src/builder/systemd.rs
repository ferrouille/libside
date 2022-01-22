use crate::config::systemd::*;
use crate::graph::GraphNodeReference;
use crate::requirements::{Requirement, Supports};
use crate::system::{NeverError, System};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::io::{BufRead, BufReader, Cursor, Write};

use super::fs::{ConfigFileData, CreateDirectory, FileWithContents};
use super::path::WillBeCreated;
use super::{path::BindPath, Chroot, Context, Mounted, Path};

pub trait SystemdUnit {
    fn name(&self) -> &str;
    fn start_dependencies(&self) -> &[GraphNodeReference];
    fn file_dependency(&self) -> GraphNodeReference;
}

#[derive(Clone)]
pub struct SandboxBuilder {
    root_dir: Path<Chroot>,
    bind_read_only_paths: Vec<String>,
    graph_dependencies: Vec<GraphNodeReference>,
}

impl SandboxBuilder {
    /// Builder for a configuration with many sandboxing options enabled by default.
    /// You should add bind_read_only_paths_push() calls for any files that need to be accessible from the chroot.
    /// You should set private_network(false) if the service needs internet access.
    /// You should call system_call_filter_push, by default system calls are filtered to @system-service. You should try and see if you can remove @privileged and @resources
    pub fn new(root_dir: &Path<Chroot>) -> SandboxBuilder {
        SandboxBuilder {
            root_dir: root_dir.clone(),
            bind_read_only_paths: Vec::new(),
            graph_dependencies: Vec::new(),
        }
    }

    pub fn build(self) -> Exec {
        let mut e = Exec::new()
            .private_tmp(true)
            .private_devices(true)
            .private_network(true)
            .protect_home(ProtectHome::Yes)
            .protect_kernel_logs(true)
            .protect_kernel_modules(true)
            .protect_kernel_tunables(true)
            .protect_system(ProtectSystem::Strict)
            .protect_clock(true)
            .protect_control_groups(true)
            .restrict_realtime(true)
            .restrict_suid_sgid(true)
            .remove_ipc(true)
            .system_call_architectures("native")
            .memory_deny_write_execute(true)
            .protect_hostname(true)
            .no_new_privileges(true)
            .lock_personality(true)
            .private_users(false)
            .restrict_namespaces_push("true")
            // Restricts sockets to unix domain sockets
            .restrict_address_families_push("AF_UNIX")
            .capability_bounding_set_push("")
            // Limit the system calls to just @system-service
            .system_call_filter_push("@system-service")
            .system_call_error_number("EPERM")
            // RW just for the current user
            .u_mask("0066")
            .root_directory(self.root_dir.clone())
            .bind_read_only_paths_push("/usr/lib /usr/lib64 /lib /lib64")
            .temporary_file_system_push("/var/tmp");

        for p in self.bind_read_only_paths {
            e = e.bind_read_only_paths_push(p);
        }

        e.graph_dependencies.extend(self.graph_dependencies);

        e
    }

    pub fn convert_path_into(&self, path: &Path<Chroot>) -> Path<Mounted> {
        path.rebase_on(&self.root_dir)
    }

    pub fn bind_read_only_path(&mut self, path: BindPath) -> Path<Mounted> {
        let (config, path, dependencies) = path.build(&self.root_dir.full_path());
        self.bind_read_only_paths.push(config);
        self.graph_dependencies.extend(dependencies);

        path
    }
}

pub struct SystemdService {
    name: String,
    full_name: String,
    file_dependency: GraphNodeReference,
    pub(crate) start_dependencies: Vec<GraphNodeReference>,
    override_dir: Option<Path<WillBeCreated>>,
}

impl SystemdService {
    pub fn from_name_unchecked(
        name: &str,
        file_dependency: GraphNodeReference,
        start_dependencies: Vec<GraphNodeReference>,
    ) -> SystemdService {
        SystemdService {
            name: name.to_owned(),
            full_name: format!("{}.service", name),
            file_dependency,
            start_dependencies,
            override_dir: None,
        }
    }

    pub fn name(&self) -> &str {
        self.full_name.as_str()
    }

    pub fn set_timer<R: Requirement>(
        self,
        context: &mut Context<R>,
        data: TimerData,
    ) -> SystemdTimer
    where
        R: Supports<FileWithContents> + Supports<InstallServices> + Supports<EnableService>,
    {
        let disabled_service = EnableService::disable(context, &self);
        let timer = data.install(context, &self.name, disabled_service);

        timer
    }

    pub fn service_override<R: Requirement>(
        &mut self,
        context: &mut Context<R>,
        override_name: &str,
        data: ServiceData,
    ) where
        R: Supports<CreateDirectory> + Supports<FileWithContents> + Supports<InstallServices>,
    {
        let override_dir = self.override_dir.get_or_insert_with(|| {
            let dir = context.existing("/etc/systemd/system/");
            dir.make_dir(context, format!("{}.service.d", self.name))
        });
        let override_file = ConfigFileData {
            path: override_dir
                .join(format!("{}.conf", override_name))
                .full_path(),
            contents: data.to_vec().unwrap(),
            path_dependency: override_dir.node,
            extra_dependencies: vec![self.file_dependency],
        }
        .create(context);
        let reload = InstallServices::run(context, &[override_file.node.unwrap()]);

        self.start_dependencies.push(reload);
        self.start_dependencies.extend(data.dependencies().copied());
    }

    pub fn add_start_dependencies<I: IntoIterator<Item = GraphNodeReference>>(&mut self, dep: I) {
        self.start_dependencies.extend(dep);
    }

    pub fn restart<R: Requirement + Supports<ServiceRunning>>(
        &self,
        context: &mut Context<R>,
    ) -> GraphNodeReference {
        ServiceRunning::restart(context, self)
    }
}

impl SystemdUnit for SystemdService {
    fn name(&self) -> &str {
        &self.name()
    }

    fn start_dependencies(&self) -> &[GraphNodeReference] {
        &self.start_dependencies
    }

    fn file_dependency(&self) -> GraphNodeReference {
        self.file_dependency
    }
}

pub struct ServiceData {
    pub unit: Unit,
    pub install: Install,
    pub service: Service,
    pub exec: Exec,
    pub resource_control: ResourceControl,
}

impl ServiceData {
    fn to_vec(&self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let f = &mut data;

        writeln!(f, "[Unit]")?;
        writeln!(f, "{}", self.unit)?;

        writeln!(f, "[Install]")?;
        writeln!(f, "{}", self.install)?;

        writeln!(f, "[Service]")?;
        writeln!(f, "{}", self.exec)?;
        writeln!(f, "{}", self.service)?;
        writeln!(f, "{}", self.resource_control)?;

        Ok(data)
    }

    fn dependencies<'a>(&'a self) -> impl Iterator<Item = &'a GraphNodeReference> {
        self.unit
            .graph_dependencies
            .iter()
            .chain(self.install.graph_dependencies.iter())
            .chain(self.service.graph_dependencies.iter())
            .chain(self.exec.graph_dependencies.iter())
            .chain(self.resource_control.graph_dependencies.iter())
    }

    pub fn install<R: Requirement + Supports<FileWithContents>>(
        self,
        context: &mut Context<R>,
        name: &str,
    ) -> SystemdService {
        let dir = context.existing("/etc/systemd/system/");
        let created_file = ConfigFileData {
            path: dir.join(&format!("{}.service", name)).full_path(),
            contents: self.to_vec().unwrap(),
            path_dependency: dir.node,
            extra_dependencies: Vec::new(),
        }
        .create(context);

        let deps = std::iter::once(created_file.node.unwrap())
            .chain(self.dependencies().copied())
            .collect();
        SystemdService::from_name_unchecked(name, created_file.graph_node().unwrap(), deps)
    }
}

pub struct SystemdTimer {
    name: String,
    file_dependency: GraphNodeReference,
    pub(crate) start_dependencies: Vec<GraphNodeReference>,
}

impl SystemdTimer {
    pub(crate) fn new(
        name: &str,
        file_dependency: GraphNodeReference,
        start_dependencies: Vec<GraphNodeReference>,
    ) -> SystemdTimer {
        SystemdTimer {
            name: name.to_owned(),
            file_dependency,
            start_dependencies,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn restart<R: Requirement + Supports<ServiceRunning>>(
        &self,
        context: &mut Context<R>,
    ) -> GraphNodeReference {
        ServiceRunning::restart(context, self)
    }
}

impl SystemdUnit for SystemdTimer {
    fn name(&self) -> &str {
        &self.name
    }

    fn start_dependencies(&self) -> &[GraphNodeReference] {
        &self.start_dependencies
    }

    fn file_dependency(&self) -> GraphNodeReference {
        self.file_dependency
    }
}

pub struct TimerData {
    pub unit: Unit,
    pub install: Install,
    pub timer: Timer,
}

impl TimerData {
    fn to_vec(&self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let f = &mut data;

        writeln!(f, "[Unit]")?;
        writeln!(f, "{}", self.unit)?;

        writeln!(f, "[Install]")?;
        writeln!(f, "{}", self.install)?;

        writeln!(f, "[Timer]")?;
        writeln!(f, "{}", self.timer)?;

        Ok(data)
    }

    fn dependencies<'a>(&'a self) -> impl Iterator<Item = &'a GraphNodeReference> {
        self.unit
            .graph_dependencies
            .iter()
            .chain(self.install.graph_dependencies.iter())
            .chain(self.timer.graph_dependencies.iter())
    }

    fn install<R: Requirement + Supports<FileWithContents> + Supports<EnableService>>(
        self,
        context: &mut Context<R>,
        name: &str,
        disabled_service: GraphNodeReference,
    ) -> SystemdTimer {
        let full_name = format!("{}.timer", name);
        let dir = context.existing("/etc/systemd/system/");
        let created_file = ConfigFileData {
            path: dir.join(&full_name).full_path(),
            contents: self.to_vec().unwrap(),
            path_dependency: dir.node,
            extra_dependencies: Vec::new(),
        }
        .create(context);

        let deps = std::iter::once(created_file.node.unwrap())
            .chain(self.dependencies().copied())
            .chain(std::iter::once(disabled_service))
            .collect();

        let mut timer = SystemdTimer::new(&full_name, created_file.graph_node().unwrap(), deps);
        let node = EnableService::enable(context, &timer);
        timer.start_dependencies.push(node);

        timer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceRunning {
    name: String,
}

impl ServiceRunning {
    pub fn restart<U: SystemdUnit, R: Requirement + Supports<ServiceRunning>>(
        context: &mut Context<R>,
        unit: &U,
    ) -> GraphNodeReference {
        context.add_node(
            ServiceRunning {
                name: unit.name().to_string(),
            },
            unit.start_dependencies(),
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemdError<S: System> {
    #[error("unable to execute systemctl: {0}")]
    FailedToStart(S::CommandError),

    #[error("systemctl failed: {0}")]
    Unsuccessful(String),
}

#[derive(Debug, thiserror::Error)]
#[error("unable to execute apt-get: {0}")]
pub struct CheckError<S: System>(S::CommandError);

impl Requirement for ServiceRunning {
    type CreateError<S: System> = SystemdError<S>;
    type ModifyError<S: System> = SystemdError<S>;
    type DeleteError<S: System> = SystemdError<S>;
    type HasBeenCreatedError<S: System> = CheckError<S>;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let result = system
            .execute_command("systemctl", &["start", &self.name])
            .map_err(SystemdError::FailedToStart)?;

        result
            .successful()
            .map_err(|(stdout, stderr)| SystemdError::Unsuccessful(format!("{stdout}\n{stderr}")))
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        let result = system
            .execute_command("systemctl", &["restart", &self.name])
            .map_err(SystemdError::FailedToStart)?;

        result
            .successful()
            .map_err(|(stdout, stderr)| SystemdError::Unsuccessful(format!("{stdout}\n{stderr}")))
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let result = system
            .execute_command("systemctl", &["stop", &self.name])
            .map_err(SystemdError::FailedToStart)?;

        result
            .successful()
            .map_err(|(stdout, stderr)| SystemdError::Unsuccessful(format!("{stdout}\n{stderr}")))
    }

    fn pre_existing_delete<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<(), Self::DeleteError<S>> {
        self.delete(system)
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let result = system
            .execute_command("systemctl", &["is-active", &self.name])
            .map_err(CheckError)?;
        Ok(result.is_success())
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
    }

    fn supports_modifications(&self) -> bool {
        true
    }

    fn can_undo(&self) -> bool {
        true
    }

    fn may_pre_exist(&self) -> bool {
        true
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "service_status";
}

impl Display for ServiceRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "running({})", self.name)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallServices;

impl InstallServices {
    pub fn run<R: Requirement + Supports<InstallServices>>(
        context: &mut Context<R>,
        dependencies: &[GraphNodeReference],
    ) -> GraphNodeReference {
        context.add_node(InstallServices, dependencies)
    }

    fn exec<S: System>(&self, system: &mut S) -> Result<(), SystemdError<S>> {
        let result = system
            .execute_command("systemctl", &["daemon-reload"])
            .map_err(SystemdError::FailedToStart)?;
        // TODO: Better error
        result.successful().map_err(|(stdout, stderr)| {
            SystemdError::Unsuccessful(format!("{stdout}\n{stderr}"))
        })?;

        let result = system
            .execute_command(
                "systemctl",
                &[
                    "list-units",
                    "--all",
                    "--state=not-found",
                    "--no-legend",
                    "--plain",
                    "--no-pager",
                    "--full",
                ],
            )
            .map_err(SystemdError::FailedToStart)?;
        for line in BufReader::new(&mut Cursor::new(&result.stdout_as_str())).lines() {
            let line = line.unwrap();
            let mut i = line.split_whitespace();
            let name = i.next().unwrap(); // TODO: Handle error
            let load = i.next().unwrap();
            let active = i.next().unwrap();
            let sub = i.next().unwrap();

            if load == "not-found" && active == "inactive" && sub == "running" {
                println!("  not-found: {}", name);
                let result = system
                    .execute_command("systemctl", &["stop", &name])
                    .map_err(SystemdError::FailedToStart)?;

                // TODO: Better error
                result.successful().map_err(|(stdout, stderr)| {
                    SystemdError::Unsuccessful(format!("{stdout}\n{stderr}"))
                })?;
            }
        }

        Ok(())
    }
}

impl Requirement for InstallServices {
    type CreateError<S: System> = SystemdError<S>;
    type ModifyError<S: System> = SystemdError<S>;
    type DeleteError<S: System> = SystemdError<S>;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        self.exec(system)
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        self.exec(system)
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        self.exec(system)
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        Ok(false)
    }

    fn affects(&self, _other: &Self) -> bool {
        false
    }

    fn supports_modifications(&self) -> bool {
        true
    }

    fn can_undo(&self) -> bool {
        false
    }

    fn may_pre_exist(&self) -> bool {
        false
    }

    fn verify<S: System>(&self, _system: &mut S) -> Result<bool, ()> {
        Ok(true)
    }

    const NAME: &'static str = "install_services";
}

impl Display for InstallServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "install-services")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnableService {
    name: String,
    disable: bool,
}

impl EnableService {
    pub fn enable<U: SystemdUnit, R: Requirement + Supports<EnableService>>(
        context: &mut Context<R>,
        unit: &U,
    ) -> GraphNodeReference {
        context.add_node(
            EnableService {
                name: unit.name().to_string(),
                disable: false,
            },
            &[unit.file_dependency()],
        )
    }

    pub fn disable<U: SystemdUnit, R: Requirement + Supports<EnableService>>(
        context: &mut Context<R>,
        unit: &U,
    ) -> GraphNodeReference {
        context.add_node(
            EnableService {
                name: unit.name().to_string(),
                disable: true,
            },
            unit.start_dependencies(),
        )
    }

    fn keyword(b: bool) -> &'static str {
        if b {
            "disable"
        } else {
            "enable"
        }
    }
}

impl Requirement for EnableService {
    type CreateError<S: System> = NeverError;
    type ModifyError<S: System> = NeverError;
    type DeleteError<S: System> = NeverError;
    type HasBeenCreatedError<S: System> = NeverError;

    fn create<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::CreateError<S>> {
        let result = system
            .execute_command("systemctl", &[Self::keyword(self.disable), &self.name])
            .unwrap();
        assert!(result.is_success());

        Ok(())
    }

    fn modify<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::ModifyError<S>> {
        let result = system
            .execute_command("systemctl", &[Self::keyword(self.disable), &self.name])
            .unwrap();
        assert!(result.is_success());

        Ok(())
    }

    fn delete<S: crate::system::System>(&self, system: &mut S) -> Result<(), Self::DeleteError<S>> {
        let result = system
            .execute_command("systemctl", &[Self::keyword(!self.disable), &self.name])
            .unwrap();
        assert!(result.is_success());

        Ok(())
    }

    fn pre_existing_delete<S: crate::system::System>(
        &self,
        _system: &mut S,
    ) -> Result<(), Self::DeleteError<S>> {
        Ok(())
    }

    fn has_been_created<S: crate::system::System>(
        &self,
        system: &mut S,
    ) -> Result<bool, Self::HasBeenCreatedError<S>> {
        let result = system
            .execute_command("systemctl", &["is-enabled", &self.name])
            .unwrap();
        let s = result.stdout_as_str().trim();

        Ok(if self.disable {
            s == "disabled" || s == "static"
        } else {
            s == "enabled"
        })
    }

    fn affects(&self, other: &Self) -> bool {
        self.name == other.name
    }

    fn supports_modifications(&self) -> bool {
        false
    }

    fn can_undo(&self) -> bool {
        true
    }

    fn may_pre_exist(&self) -> bool {
        true
    }

    fn verify<S: System>(&self, system: &mut S) -> Result<bool, ()> {
        Ok(self.has_been_created(system).unwrap())
    }

    const NAME: &'static str = "service_enabled";
}

impl Display for EnableService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", Self::keyword(self.disable), self.name)
    }
}
