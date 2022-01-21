use std::fmt::Write;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use libside::builder::apt::{AptInstall, AptPackage};
use libside::builder::fs::*;
use libside::builder::mysql::*;
use libside::builder::nginx::Nginx;
use libside::builder::path::{Bindable, Existing, Exposed, Path, SharedConfig};
use libside::builder::php_fpm::*;
use libside::builder::systemd::*;
use libside::builder::users::*;
use libside::builder::{AsParam, Builder, Context};
use libside::config::systemd::*;
use libside::graph::GraphNodeReference;
use libside::requirements::{Requirement, Supports};
use libside::secrets::keys::AsymmetricKey;
use libside::secrets::password::{Alphanumeric, Password};
use libside::{config_file, SiDe};
use libside::{generic_apt_package, requirements};
use serde::{Deserialize, Serialize};

generic_apt_package!(Rsync => "rsync");
generic_apt_package!(Ssh => "ssh");

#[derive(Deserialize)]
enum Config {
    #[serde(rename = "www")]
    Www(Www),

    #[serde(rename = "binary")]
    Binary(Binary),

    #[serde(rename = "backup")]
    Backup(Backup),
}

#[derive(Deserialize)]
struct Www {
    path: String,
    hostname: String,
    document_root: Option<String>,

    #[serde(default)]
    php: bool,

    #[serde(default)]
    database: Option<DatabaseConfig>,
}

#[derive(Deserialize)]
struct DatabaseConfig {
    name: String,
    user: String,

    #[serde(default)]
    backup: bool,
}

#[derive(Deserialize)]
struct Binary {
    path: String,
    executable: String,
    arguments: String,
    #[serde(default)]
    network_access: bool,
}

#[derive(Deserialize)]
struct Backup {
    #[serde(default)]
    rsync: Option<BackupRemote>,
}

#[derive(Clone, Deserialize)]
struct BackupRemote {
    host: String,
    path: String,
    known_host: String,
    user: String,
}

struct Demo;

struct DemoData {
    fpm_socks_group: Group,
    nginx_user: (User, Group),
    nginx: Nginx,
    nginx_service: SystemdService,
    nginx_sites: Path<SharedConfig>,
    nginx_config_dir: Path<SharedConfig>,
    php_fpm: Option<PhpFpm<Php80>>,
    sites: Vec<(Path<Exposed>, Option<(Path<Existing>, GraphNodeReference)>)>,
    mysql: Option<MySqlData>,
    backup: Option<BackupData>,
}

struct MySqlData {
    mysql: MariaDb,
    service: MySqlService,
}

struct BackupData {
    databases: Vec<(Database, Path<libside::builder::path::Backup>)>,
    sync_to: Option<BackupRemote>,
}

impl BackupData {
    pub fn new() -> BackupData {
        BackupData {
            databases: Vec::new(),
            sync_to: None,
        }
    }
}

impl MySqlData {
    pub fn create<R: Requirement>(context: &mut Context<R>) -> MySqlData
    where
        R: Supports<AptInstall> + Supports<ServiceRunning>,
    {
        let mysql = MariaDb::install(context);
        let service = mysql.default_service();

        MySqlData { mysql, service }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("TODO error")]
struct BuildError;

impl From<()> for BuildError {
    fn from(_: ()) -> Self {
        BuildError
    }
}

#[derive(Serialize)]
struct Status {
    nginx_running: bool,
}

impl Builder for Demo {
    type PackageConfig = Config;
    type Data = DemoData;
    type Requirement = requirements!(
        CreateDirectory,
        FileWithContents,
        CreateUser,
        CreateGroup,
        AptInstall,
        ServiceRunning,
        CreateMySqlDatabase,
        CreateMySqlUser,
        CreateMySqlGrant,
        InstallServices,
        Delete,
        EnableService,
        Chown,
        Chmod,
    );
    type BuildError = BuildError;

    fn start_build(
        &self,
        context: &mut libside::builder::Context<Self::Requirement>,
    ) -> Result<Self::Data, Self::BuildError> {
        let root = context.config_root();
        let nginx_config_dir = root.make_dir(context, "nginx");
        let nginx_sites = nginx_config_dir.make_dir(context, "sites");

        let dpkg_dir = context.existing("/etc/dpkg/dpkg.cfg.d/");
        config_file!("demo-data/dpkg/01_nodoc")
            .in_dir(&dpkg_dir)
            .create(context);

        let fpm_socks_group = Group::add(context, "fpm-socks", true);
        let nginx_user = User::add(context, "nginx-www", |c| c.add_group(&fpm_socks_group));

        let nginx = Nginx::install(context);
        Ok(DemoData {
            fpm_socks_group,
            nginx_service: nginx.default_service(),
            nginx,
            nginx_user,
            nginx_sites,
            nginx_config_dir,
            php_fpm: None,
            mysql: None,
            backup: None,
            sites: Vec::new(),
        })
    }

    fn build_package(
        &self,
        package: &libside::builder::Package<Self::PackageConfig>,
        context: &mut libside::builder::Context<Self::Requirement>,
        data: &mut Self::Data,
    ) -> Result<(), Self::BuildError> {
        match &package.config() {
            Config::Www(www) => {
                let base_path = package.root().join(&www.path)?;
                let exposed_base_path = context.expose(&base_path);
                let document_root = www
                    .document_root
                    .as_ref()
                    .map(|r| exposed_base_path.join(r))
                    .unwrap_or_else(|| exposed_base_path.clone());

                if www.php {
                    let php_fpm = data.php_fpm.get_or_insert_with(|| {
                        // disable the default php-fpm service, but don't remove it.
                        // apt packages will fail to install if they can't restart the php-fpm service.
                        let php_fpm = PhpFpm::<Php80>::install(context);
                        php_fpm.default_service().service_override(
                            context,
                            "disable",
                            ServiceData {
                                unit: Unit::new(),
                                install: Install::new(),
                                service: Service::new()
                                    .service_type(ServiceType::Notify)
                                    .exec_start_push("/bin/true")
                                    .exec_start_pre_push("")
                                    .exec_start_post_push("")
                                    .exec_reload_push("/bin/true"),
                                exec: Exec::new(),
                                resource_control: ResourceControl::new(),
                            },
                        );

                        php_fpm
                    });
                    let fpm_binary = php_fpm.binary();

                    let fpm_name = format!("php8.0-fpm-{}", www.hostname);
                    let (fpm_user, fpm_group) =
                        User::add(context, &fpm_name, |c| c.add_group(&data.fpm_socks_group));

                    let fpm_root = context.create_chroot("fpm");
                    let php_root = fpm_root.make_dir(context, "php-data");
                    let extra_config = context.config_root().make_dir(context, "config");

                    let mut sb = SandboxBuilder::new(&fpm_root);
                    let php_files = sb.bind_read_only_path(
                        exposed_base_path
                            .bind()
                            .in_dir(&sb.convert_path_into(&php_root.join("phpfiles"))),
                    );

                    let real_php_root = sb.convert_path_into(&php_root);
                    let mounted_config = sb.convert_path_into(&fpm_root.join("config"));
                    let mounted_root = sb.convert_path_into(&fpm_root);
                    let real_extra_config =
                        sb.bind_read_only_path(extra_config.bind().in_dir(&mounted_config));

                    let listen_sock = context
                        .existing("/run")
                        .join(&fpm_name)
                        .join("php-fpm.sock");
                    let fpm_config = &context.config_root().make_file(
                        context,
                        config_file!("demo-data/php-fpm/php-fpm.conf"
                            listen_sock: &listen_sock,
                            user: &fpm_user,
                            group: &fpm_group,
                            sock_group: &data.fpm_socks_group,
                            chroot: &real_php_root,
                            extra_config_path: &real_extra_config,
                        ),
                    );

                    sb.bind_read_only_path(fpm_config.bind());

                    if let Some(db_config) = &www.database {
                        if data.mysql.is_none() {
                            data.mysql = Some(MySqlData::create(context));
                        }

                        let mounted_unix_socket = sb.convert_path_into(&php_root.join("mysql"));

                        let mysql = &mut data.mysql.as_mut().unwrap();
                        let running = mysql.service.run(context);
                        let db = running.create_database(context, &db_config.name);
                        let password: Password<32, Alphanumeric> = context.secret("mysql_password");
                        let user = running.create_user(context, &db_config.user, password.as_ref());
                        user.grant(
                            context,
                            [
                                Privilege::Select,
                                Privilege::Insert,
                                Privilege::Update,
                                Privilege::Delete,
                            ]
                            .iter()
                            .copied()
                            .collect(),
                            &db,
                        );

                        let s = running.unix_socket();
                        let _socket_path = sb.bind_read_only_path(
                            s.parent().unwrap().bind().in_dir(&mounted_unix_socket),
                        );

                        let file = extra_config.make_file(context, config_file!("demo-data/php-fpm/mysql-env.conf"
                            socket: format!("/mysql/{}", s.file_name().unwrap().to_str().unwrap()),
                            database: db.to_string(),
                            user: user.to_string(),
                            pass: password.as_ref(),
                        ));

                        sb.bind_read_only_path(file.bind().in_dir(&real_extra_config));

                        if db_config.backup {
                            if data.backup.is_none() {
                                data.backup = Some(BackupData::new());
                            }

                            let dir = context.backup_root().make_dir(context, "mysql");
                            data.backup.as_mut().unwrap().databases.push((db, dir));
                        }
                    }

                    let mut service = ServiceData {
                        unit: Unit::new()
                            .description(format!("PHP for {}", www.hostname))
                            .after_push("network.target"),
                        install: Install::new().wanted_by_push("multi-user.target"),
                        service: Service::new()
                            .service_type(ServiceType::Notify)
                            .p_i_d_file(format!("/run/{}/fpm.pid", fpm_name))
                            .exec_start_push(format!(
                                "/usr/sbin/php-fpm8.0 --nodaemonize --fpm-config {}",
                                fpm_config.as_param()
                            ))
                            .exec_reload_push("/bin/kill -USR2 $MAINPID"),
                        exec: sb
                            .build()
                            .bind_read_only_paths_push(
                                "/usr/sbin/php-fpm8.0 /etc/php /run /etc/passwd /etc/group",
                            )
                            .protect_proc(ProtectProc::Invisible)
                            .proc_subset(ProcSubset::Pid)
                            // Limit the system calls to what is actually reeded
                            .system_call_filter_push("~@resources")
                            .system_call_filter_push("chroot")
                            // We start php-fpm as root, and then have it chroot into the scripts directory and setuid/setgid to the right user.
                            .capability_bounding_set_push(
                                "CAP_CHOWN CAP_SETGID CAP_SETUID CAP_SYS_CHROOT",
                            )
                            // TODO: Custom type to create bind paths. Last component in path may be non-existant, so /existing/existing/existing/nonexistant
                            .temporary_file_system_push("/var")
                            .runtime_directory_push(&fpm_name)
                            .logs_directory_push(&fpm_name),
                        resource_control: ResourceControl::new()
                            .ip_address_deny("any")
                            .device_policy(DevicePolicy::Strict),
                    }
                    .install(context, &fpm_name);
                    service.add_start_dependencies(fpm_binary.graph_node());
                    service.add_start_dependencies(fpm_config.graph_node());
                    service.add_start_dependencies(fpm_root.graph_node());
                    service.add_start_dependencies(php_root.graph_node());
                    service.add_start_dependencies([fpm_user.graph_node(), fpm_group.graph_node()]);

                    if let Some(_) = &www.database {
                        let pdo = PhpMySql::install(context);
                        service.add_start_dependencies([pdo.graph_node()]);
                    }

                    let started = ServiceRunning::restart(context, &service);
                    let site_file = data.nginx_sites.make_file(context, config_file!("demo-data/nginx/php-site"
                        document_root: document_root.clone(),
                        // php-fpm is configured to chroot into /php-data, so we need to strip /php-data from the path we're going to pass to php-fpm
                        php_root: PathBuf::from("/").join(PathBuf::from(php_files.to_string()).strip_prefix("/php-data").unwrap()).display().to_string(),
                        server_name: &www.hostname,
                        fpm_socket: &listen_sock,
                    ).rename(package.name()));

                    data.nginx_service
                        .add_start_dependencies(site_file.graph_node());
                    data.sites
                        .push((document_root, Some((listen_sock, started))));
                } else {
                    let site_file = data.nginx_sites.make_file(
                        context,
                        config_file!("demo-data/nginx/html-site"
                            document_root: document_root.clone(),
                            server_name: &www.hostname,
                        )
                        .rename(package.name()),
                    );

                    data.nginx_service
                        .add_start_dependencies(site_file.graph_node());
                    data.sites.push((document_root, None));
                }
            }
            Config::Binary(binary) => {
                let root = context.create_chroot("binary");

                let exposed_path = context.expose(&context.package_root().join(&binary.path)?);

                let mut sb = SandboxBuilder::new(&root);
                let mounted_path = sb.bind_read_only_path(exposed_path.bind());
                let binary_path = mounted_path.join(&binary.executable);

                let (user, group) = User::add(context, package.name(), |c| c);

                let exec = sb.build();
                let exec = if binary.network_access {
                    exec.private_network(false)
                        .restrict_address_families_push("AF_INET AF_INET6")
                        .capability_bounding_set_push("CAP_NET_BIND_SERVICE")
                        .ambient_capabilities_push("CAP_NET_BIND_SERVICE")
                        .system_call_filter_push("~@resources @privileged")
                        .system_call_filter_push("@network-io")
                } else {
                    exec.system_call_filter_push("~@resources")
                };

                let service = ServiceData {
                    unit: Unit::new()
                        .description(package.name().to_string())
                        .after_push("network.target"),
                    install: Install::new().wanted_by_push("multi-user.target"),
                    service: Service::new()
                        .service_type(ServiceType::Simple)
                        .exec_start_push(format!(
                            "{} {}",
                            binary_path.as_param(),
                            binary.arguments
                        )),
                    exec: exec
                        .user(&user)
                        .group(&group)
                        .protect_proc(ProtectProc::Invisible)
                        .proc_subset(ProcSubset::Pid)
                        .temporary_file_system_push("/var")
                        .runtime_directory_push(package.name())
                        .logs_directory_push(package.name()),
                    resource_control: ResourceControl::new().device_policy(DevicePolicy::Strict),
                }
                .install(context, package.name());

                let _started = ServiceRunning::restart(context, &service);
            }
            Config::Backup(backup) => {
                if data.backup.is_none() {
                    data.backup = Some(BackupData::new());
                }

                data.backup.as_mut().unwrap().sync_to = backup.rsync.clone();
            }
        }

        Ok(())
    }

    fn finish_build(
        &self,
        context: &mut libside::builder::Context<Self::Requirement>,
        mut data: Self::Data,
    ) -> Result<(), Self::BuildError> {
        let nginx = data.nginx;
        let mut nginx_service = data.nginx_service;
        let nginx_conf_file = data.nginx_config_dir.make_file(
            context,
            config_file!("demo-data/nginx/nginx.conf"
                sites_path: data.nginx_sites.clone(),
            ),
        );

        let fastcgi_params = data
            .nginx_config_dir
            .make_file(context, config_file!("demo-data/nginx/fastcgi_params"));

        let nginx_root = context.create_chroot("nginx");
        let mut sb = SandboxBuilder::new(&nginx_root);

        let mut deps = Vec::new();
        for (document_root, fpm_sock) in data.sites.iter() {
            sb.bind_read_only_path(document_root.bind());
            if let Some((sock, node)) = fpm_sock {
                sb.bind_read_only_path(sock.bind());
                deps.push(*node);
            }
        }

        sb.bind_read_only_path(data.nginx_config_dir.bind());

        let exec = sb
            .build()
            .private_network(false)
            // Allow internet sockets so nginx can serve webpages
            .restrict_address_families_push("AF_INET AF_INET6")
            // Grant CAP_NET_BIND_SERVICE to allow nginx to bind port 80/443
            .capability_bounding_set_push("CAP_NET_BIND_SERVICE")
            .ambient_capabilities_push("CAP_NET_BIND_SERVICE")
            .system_call_filter_push("~@resources @privileged")
            .system_call_filter_push("@network-io")
            .user(&data.nginx_user.0)
            .group(&data.nginx_user.1)
            .runtime_directory_push("nginx")
            .logs_directory_push("nginx")
            .bind_read_only_paths_push(
                "/usr/sbin/nginx /run/ /etc/nginx /usr/share/nginx/ /etc/passwd /etc/group",
            )
            .temporary_file_system_push("/var/lib/nginx/body")
            .temporary_file_system_push("/var/lib/nginx/proxy")
            .temporary_file_system_push("/var/lib/nginx/fastcgi")
            .temporary_file_system_push("/var/lib/nginx/uwsgi")
            .temporary_file_system_push("/var/lib/nginx/scgi");

        nginx_service.service_override(context, "99-overrides", ServiceData {
            unit: Unit::new(),
            install: Install::new(),
            service: Service::new()
                .p_i_d_file("/run/nginx/nginx.pid")
                .reset_exec_start()
                .exec_start_push(format!("/usr/sbin/nginx -c {} -g 'daemon on; master_process on;'", nginx_conf_file.as_param()))
                .reset_exec_start_pre()
                .exec_start_pre_push(format!("/usr/sbin/nginx -c {} -t -q -g 'daemon on; master_process on;'", nginx_conf_file.as_param()))
                .reset_exec_reload()
                .exec_reload_push(format!("/usr/sbin/nginx -c {} -g 'daemon on; master_process on;' -s reload", nginx_conf_file.as_param()))
                .reset_exec_stop()
                .exec_stop_push("-/sbin/start-stop-daemon --quiet --stop --retry QUIT/5 --pidfile /run/nginx/nginx.pid"),
            exec,
            resource_control: ResourceControl::new()
                .device_allow_push("")
                .device_policy(DevicePolicy::Strict),
        });
        nginx_service.add_start_dependencies(nginx.binary().graph_node());
        nginx_service.add_start_dependencies(nginx_conf_file.graph_node());
        nginx_service.add_start_dependencies(fastcgi_params.graph_node());
        nginx_service.add_start_dependencies(deps);

        ServiceRunning::restart(context, &nginx_service);

        if let Some(backup) = data.backup {
            let mysql_group = data.mysql.as_ref().map(|m| m.mysql.mysql_group());
            let (backup_user, backup_group) = User::add(context, "backup-service", |c| {
                if let Some(group) = mysql_group.as_ref() {
                    c.add_group(group);
                }

                c
            });

            let mut script = String::new();
            if let Some(mysql) = &mut data.mysql {
                let running = mysql.service.run(context);
                let pass = context.secret::<Password<32, Alphanumeric>>("backup_mysql_password");
                let mysql_user = running.create_user(context, "backup", pass.as_ref());
                for (db, _) in backup.databases.iter() {
                    mysql_user.grant(
                        context,
                        [
                            Privilege::Select,
                            Privilege::ShowView,
                            Privilege::Trigger,
                            Privilege::LockTables,
                        ]
                        .iter()
                        .copied()
                        .collect(),
                        &db,
                    );
                }

                writeln!(&mut script, "#! /bin/bash").unwrap();
                writeln!(&mut script, "DATE=$(date '+%Y-%m-%d')").unwrap();
                for (db, dir) in backup.databases.iter() {
                    dir.chown(context, &backup_user, &backup_group);
                    writeln!(
                        &mut script,
                        "mysqldump -u {} -p{} {} | gzip --fast > {}/$DATE.sql.gz",
                        mysql_user,
                        pass.as_ref(),
                        db,
                        dir
                    )
                    .unwrap();
                }
            }

            let root = context.create_chroot("backup");
            let mut sb = SandboxBuilder::new(&root);
            let runfile = context.config_root().make_file(
                context,
                ConfigFileData {
                    path: PathBuf::from("backup.sh"),
                    contents: script.into_bytes(),
                    path_dependency: None,
                    extra_dependencies: Vec::new(),
                },
            );
            let runfile = sb.bind_read_only_path(runfile.bind());
            let service = ServiceData {
                unit: Unit::new().description("backup"),
                install: Install::new(),
                service: Service::new()
                    .service_type(ServiceType::OneShot)
                    .exec_start_push(format!("/bin/bash {}", runfile)),
                exec: {
                    let mut exec = sb
                        .build()
                        .bind_read_only_paths_push("/bin")
                        .user(&backup_user)
                        .group(&backup_group)
                        .protect_proc(ProtectProc::Invisible)
                        .proc_subset(ProcSubset::Pid)
                        .temporary_file_system_push("/var");

                    for (_, dir) in backup.databases.iter() {
                        exec = exec.bind_paths_push(dir.as_param());
                    }

                    exec
                },
                resource_control: ResourceControl::new().device_policy(DevicePolicy::Strict),
            }
            .install(context, "backup");

            let timer = service.set_timer(
                context,
                TimerData {
                    unit: Unit::new(),
                    install: Install::new().wanted_by_push("timers.target"),
                    timer: Timer::new()
                        .on_calendar_push("*-*-* 01:00:00")
                        .on_calendar_push("*-*-* 09:00:00")
                        .on_calendar_push("*-*-* 17:00:00")
                        .randomized_delay_sec(&Duration::from_secs(60 * 30)),
                },
            );

            timer.restart(context);

            if let Some(rsync) = backup.sync_to {
                let root = context.create_chroot("backup-sync");
                let keypair: AsymmetricKey<4096> = context.secret("backup-sync-ssh-key");

                let dir = context.config_root().make_dir(context, "ssh");
                let private_key_file = dir.make_file(
                    context,
                    ConfigFileData {
                        path: PathBuf::from("id_rsa"),
                        contents: keypair.private_key_data().as_bytes().to_vec(),
                        path_dependency: dir.graph_node(),
                        extra_dependencies: Vec::new(),
                    },
                );

                let public_key_file = dir.make_file(
                    context,
                    ConfigFileData {
                        path: PathBuf::from("id_rsa.pub"),
                        contents: keypair.public_key_data("todo_hostname").into_bytes(),
                        path_dependency: dir.graph_node(),
                        extra_dependencies: Vec::new(),
                    },
                );

                let known_hosts_file = dir.make_file(
                    context,
                    ConfigFileData {
                        path: PathBuf::from("known_hosts"),
                        contents: rsync.known_host.into_bytes(),
                        path_dependency: dir.graph_node(),
                        extra_dependencies: Vec::new(),
                    },
                );

                for file in [private_key_file, public_key_file, known_hosts_file] {
                    file.chown(context, &backup_user, &backup_group);
                    file.chmod(context, 0o600);
                }

                let mut sb = SandboxBuilder::new(&root);
                let mounted = sb.bind_read_only_path(dir.bind());

                let key_file = mounted.join("id_rsa");
                let known_hosts_file = mounted.join("known_hosts");

                let mut script = String::new();
                writeln!(&mut script, "#! /bin/bash").unwrap();
                writeln!(
                    &mut script,
                    "rsync -rtv -e 'ssh -i {} -o \"UserKnownHostsFile={}\" -l {}' {}/ {}:{}",
                    key_file,
                    known_hosts_file,
                    rsync.user,
                    context.shared_backup_root(),
                    rsync.host,
                    rsync.path
                )
                .unwrap();

                sb.bind_read_only_path(context.shared_backup_root().bind());

                Rsync::install(context);
                Ssh::install(context);

                let runfile = context.config_root().make_file(
                    context,
                    ConfigFileData {
                        path: PathBuf::from("sync.sh"),
                        contents: script.into_bytes(),
                        path_dependency: None,
                        extra_dependencies: Vec::new(),
                    },
                );
                let runfile = sb.bind_read_only_path(runfile.bind());
                let service = ServiceData {
                    unit: Unit::new()
                        .description("sync backup to external host")
                        .after_push("backup.service"),
                    install: Install::new(),
                    service: Service::new()
                        .service_type(ServiceType::OneShot)
                        .exec_start_push(format!("/bin/bash {}", runfile)),
                    exec: sb
                        .build()
                        .bind_read_only_paths_push("/bin")
                        .bind_read_only_paths_push("/etc/passwd")
                        .user(&backup_user)
                        .group(&backup_group)
                        .protect_proc(ProtectProc::Invisible)
                        .proc_subset(ProcSubset::Pid)
                        .temporary_file_system_push("/var")
                        .restrict_address_families_push("AF_INET AF_INET6")
                        .private_network(false)
                        .private_users(false),
                    resource_control: ResourceControl::new().device_policy(DevicePolicy::Strict),
                }
                .install(context, "backup-sync");

                let timer = service.set_timer(
                    context,
                    TimerData {
                        unit: Unit::new(),
                        install: Install::new().wanted_by_push("timers.target"),
                        timer: Timer::new()
                            .on_calendar_push("*-*-* 02:00:00")
                            .on_calendar_push("*-*-* 10:00:00")
                            .on_calendar_push("*-*-* 18:00:00")
                            .randomized_delay_sec(&Duration::from_secs(60 * 30)),
                    },
                );

                timer.restart(context);
            }
        }

        Ok(())
    }
}

fn main() {
    match SiDe::run(|| Demo) {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    }
}
