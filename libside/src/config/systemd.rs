use std::time::Duration;

use crate::builder::path::{Chroot, Path};
use crate::builder::users::{Group, User};
use crate::builder::AsParam;
use crate::graph::GraphNodeReference;

macro_rules! directive_concrete_type {
    ($name:ident, multiple $ty:ty) => { Vec<$ty> };
    ($name:ident, convert $from:ty => $to:ty; $variable:ident => $e:expr) => { Option<$to> };
    ($name:ident, enum $data:tt) => { Option<$name> };
    ($name:ident, $ty:ty) => { Option<$ty> };
}

macro_rules! directive_enum {
    ($name:ident, { $($key:ident = $val:expr),* }) => {
        #[derive(Debug, Copy, Clone)]
        pub enum $name {
            $($key,)*
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(Self::$key => f.write_str($val)),*
                }
            }
        }
    }
}

macro_rules! directive_additional {
    ($name:ident, multiple $ty:ty) => {};
    ($name:ident, convert $from:ty => $to:ty; $variable:ident => $e:expr ) => {};
    ($name:ident, enum $data:tt) => {
        directive_enum! { $name, $data }
    };
    ($name:ident, $ty:ty) => {};
}

macro_rules! directive_impl {
    ($name:ident, multiple $ty:ty) => {
        paste::item! {
            pub fn [<$name:snake _push>]<I: Into<$ty>>(mut self, value: I) -> Self {
                self.[<$name:snake>].push(value.into());
                self
            }

            pub fn [<reset_ $name:snake>](mut self) -> Self {
                self.[<$name:snake>].push(Default::default());
                self
            }
        }
    };
    ($name:ident, convert $from:ty => $to:ty; $variable:ident => ($e:expr, |$deps:ident| $add_dependencies:expr)) => {
        paste::item! {
            pub fn [<$name:snake>]<'a>(mut self, value: &'a $from) -> Self {
                self.[<$name:snake>] = Some(match value {
                    $variable => {
                        match &mut self.graph_dependencies {
                            $deps => $add_dependencies,
                        }

                        $e
                    },
                });


                self
            }
        }
    };
    ($name:ident, convert $from:ty => $to:ty; $variable:ident => $e:expr) => {
        paste::item! {
            pub fn [<$name:snake>]<'a>(mut self, value: &'a $from) -> Self {
                self.[<$name:snake>] = Some(match value {
                    $variable => $e,
                });

                self
            }
        }
    };
    ($name:ident, enum $data:tt) => {
        paste::item! {
            pub fn [<$name:snake>](mut self, value: $name) -> Self {
                self.[<$name:snake>] = Some(value);

                self
            }
        }
    };
    ($name:ident, $ty:ty) => {
        paste::item! {
            pub fn [<$name:snake>]<I: Into<$ty>>(mut self, value: I) -> Self {
                self.[<$name:snake>] = Some(value.into());
                self
            }
        }
    };
}

macro_rules! directive_types {
    ($struct_name:ident []) => {};
    ($struct_name:ident [ ($name:ident $(= $real_name:expr)?, $($ty:tt)*) $($rest:tt)* ]) => {
        impl $struct_name {
            directive_impl!($name, $($ty)*);
        }

        directive_additional!($name, $($ty)*);
        directive_types!($struct_name [ $($rest)* ]);
    }
}

macro_rules! directive_fields {
    ($struct_name:ident [ $(($name:ident $(= $real_name:expr)?, $($ty:tt)*))* ]) => {
        paste::item! {
            #[derive(Clone)]
            pub struct $struct_name {
                pub graph_dependencies: Vec<GraphNodeReference>,
                $( [<$name:snake>] : directive_concrete_type!($name, $($ty)*)),*
            }
        }
    }
}

macro_rules! directive_name_str {
    ($name:ident = $real_name:expr) => {
        $real_name
    };
    ($name:ident) => {
        stringify!($name)
    };
}

macro_rules! directive_display {
    ($self:ident, $f:ident, $name:ident $(= $real_name:expr)?, multiple $ty:ty) => {
        paste::item! {
            for item in $self.[<$name:snake>].iter() {
                writeln!($f, "{}={}", directive_name_str!($name $(= $real_name)?), item)?;
            }
        }
    };
    ($self:ident, $f:ident, $name:ident $(= $real_name:expr)?, convert $from:ty => $to:ty; $variable:ident => $e:expr) => {
        paste::item! {
            if let Some(value) = &$self.[<$name:snake>] {
                writeln!($f, "{}={}", directive_name_str!($name $(= $real_name)?), value)?;
            }
        }
    };
    ($self:ident, $f:ident, $name:ident $(= $real_name:expr)?, enum $data:tt) => {
        paste::item! {
            if let Some(value) = &$self.[<$name:snake>] {
                writeln!($f, "{}={}", directive_name_str!($name $(= $real_name)?), value)?;
            }
        }
    };
    ($self:ident, $f:ident, $name:ident $(= $real_name:expr)?, $ty:ty) => {
        paste::item! {
            if let Some(value) = &$self.[<$name:snake>] {
                writeln!($f, "{}={}", directive_name_str!($name $(= $real_name)?), value)?;
            }
        }
    };
}

macro_rules! directive_displays {
    ($struct_name:ident; $self:ident; $f:ident; []) => {};
    ($struct_name:ident; $self:ident; $f:ident; [ ($name:ident $(= $real_name:expr)?, $($ty:tt)*) $($rest:tt)* ]) => {
        directive_display!($self, $f, $name $(= $real_name)?, $($ty)*);
        directive_displays!($struct_name; $self; $f; [ $($rest)* ]);
    }
}

macro_rules! directive_defaults {
    ($struct_name:ident [ $(($name:ident $(= $real_name:expr)?, $($ty:tt)*))* ]) => {
        paste::item! {
            $struct_name {
                graph_dependencies: Vec::new(),
                $( [<$name:snake>] : Default::default()),*
            }
        }
    }
}

macro_rules! directives {
    ($struct_name:ident $data:tt) => {
        directive_types! { $struct_name $data }
        directive_fields! { $struct_name $data }

        impl std::fmt::Display for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                directive_displays! { $struct_name; self; f; $data }

                Ok(())
            }
        }

        impl std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                // TODO: debug display
                directive_displays! { $struct_name; self; f; $data }

                Ok(())
            }
        }

        impl $struct_name {
            pub fn new() -> $struct_name {
                directive_defaults! { $struct_name $data }
            }
        }
    };
}

directives! {
    Exec [
        // Paths
        (WorkingDirectory, String)
        (RootDirectory, Path<Chroot>)
        (RootImage, String)
        (RootImageOptions, String)
        (RootHash, String)
        (RootHashSignature, String)
        (RootVerity, String)
        (MountApiVfs = "MountAPIVFS", bool)
        (ProtectProc, enum { NoAccess = "noaccess", Invisible = "invisible", Ptraceable = "ptraceable", Default = "default" })
        (ProcSubset, enum { All = "all", Pid = "pid" })
        (BindPaths, multiple String)
        (BindReadOnlyPaths, multiple String)
        (MountImages, multiple String)

        // Credentials
        (User, convert User => String; e => (e.as_param(), |deps| deps.push(e.graph_node())))
        (Group, convert Group => String; e => (e.as_param(), |deps| deps.push(e.graph_node())))
        (DynamicUser, bool)
        (SupplementaryGroups, multiple String)
        (PAMName, String)

        // Capabilities
        (CapabilityBoundingSet, multiple String)
        (AmbientCapabilities, multiple String)

        // Security
        (NoNewPrivileges, bool)
        (SecureBits, multiple String)
        // TODO: (SecureBits, set { KeepCaps = "keep-caps", KeepCapsLocked = "keep-caps-locked", NoSetuidFixup = "no-setuid-fixup", NoSetuidFixupLocked = "no-setuid-fixup-locked", NoRoot = "noroot", NoRootLocked = "noroot-locked"})

        // TODO: Mandatory access control

        // Process properties
        (LimitCPU, i64)
        (LimitFSIZE, i64)
        (LimitDATA, i64)
        (LimitSTACK, i64)
        (LimitCORE, i64)
        (LimitRSS, i64)
        (LimitNOFILE, i64)
        (LimitAS, i64)
        (LimitNPROC, i64)
        (LimitMEMLOCK, i64)
        (LimitLOCKS, i64)
        (LimitSIGPENDING, i64)
        (LimitMSGQUEUE, i64)
        (LimitNICE, i64)
        (LimitRTPRIO, i64)
        (LimitRTTIME, i64)

        (UMask, String)
        (CoredumpFilter, multiple String)
        (KeyringMode, String)
        (OOMScoreAdjust, i64)
        (TimerSlckNSec, i64)
        (Personality, String)
        (IgnoreSIGPIPE, bool)

        // Scheduling
        (Nice, i8)
        (CPUSchedulingPolicy, String)
        (CPUSchedulingPriority, u8)
        (CPUSchedulingResetOnFork, bool)
        (CPUAffinity, String)
        (NUMAPolicy, String)
        (NUMAMask, String)
        (IOSchedulingClass, String)
        (IOSchedulingPriority, String)

        // Sandboxing
        (ProtectSystem, enum { No = "false", Yes = "true", Full = "full", Strict = "strict" })
        (ProtectHome, enum { No = "false", Yes = "true", ReadOnly = "read-only", TmpFs = "tmpfs" })
        (RuntimeDirectory, multiple String)
        (StateDirectory, multiple String)
        (CacheDirectory, multiple String)
        (LogsDirectory, multiple String)
        (ConfigurationDirectory, multiple String)
        // TODO: Only allow these to be set if a directory is also set
        (RuntimeDirectoryMode, String)
        (StateDirectoryMode, String)
        (CacheDirectoryMode, String)
        (LogsDirectoryMode, String)
        (ConfigurationDirectoryMode, String)
        (RuntimeDirectoryPreserve, enum { No = "false", Yes = "true", Restart = "restart" })
        (TimeoutCleanSec, String)
        (ReadWritePaths, multiple String)
        (ReadOnlyPaths, multiple String)
        (InaccessiblePaths, multiple String)
        (TemporaryFileSystem, multiple String)
        (PrivateTmp, bool)
        (PrivateDevices, bool)
        (PrivateNetwork, bool)
        (NetworkNamespacePath, String)
        (PrivateUsers, bool)
        (ProtectHostname, bool)
        (ProtectClock, bool)
        (ProtectKernelTunables, bool)
        (ProtectKernelModules, bool)
        (ProtectKernelLogs, bool)
        (ProtectControlGroups, bool)
        (RestrictAddressFamilies, multiple String)
        (RestrictNamespaces, multiple String)
        (LockPersonality, bool)
        (MemoryDenyWriteExecute, bool)
        (RestrictRealtime, bool)
        (RestrictSuidSgid = "RestrictSUIDSGID", bool)
        (RemoveIpc = "RemoveIPC", bool)
        (PrivateMounts, bool)
        (MountFlags, enum { Shared = "shared", Slave = "slave", Private = "private" })

        // System call filtering
        (SystemCallFilter, multiple String)
        (SystemCallErrorNumber, String)
        (SystemCallArchitectures, String)
        (SystemCallLog, String)

        // Environment
        (Environment, multiple String)
        (EnvironmentFile, String)
        (PassEnvironment, multiple String)
        (UnsetEnvironment, multiple String)

        // Logging and standard input/output
        (StandardInput, String)
        (StandardOutput, String)
        (StandardError, String)
        (StandardInputText, String)
        (StandardInputData, String)
        (LogLevelMax, String)
        (LogExtraFields, String)
        (LogRateLimitIntervalSec, String)
        (LogRateLimitBurst, String)
        (LogNamespace, String)
        (SyslogIdentifier, String)
        (SyslogFacility, String)
        (SyslogLevel, String)
        (SyslogLevelPrefix, String)
        (TTYPath, String)
        (TTYReset, String)
        (TTYVHangup, String)
        (TTYVDisallocate, String)

        // Credentials
        (LoadCredential, String)
        (SetCredential, String)

        // Not implemented: System V compatibility
    ]
}

directives! {
    Unit [
        (Description, String)
        (Documentation, String)
        (Wants, multiple String)
        (Requires, multiple String)
        (Requisite, multiple String)
        (BindsTo, multiple String)
        (PartOf, multiple String)
        (Conflicts, multiple String)
        (Before, multiple String)
        (After, multiple String)
        (OnFailure, String)
        (PropagatesReloadTo, String)
        (ReloadPropagatedFrom, String)
        (JoinsNamespaceOf, String)
        (RequiresMountsFor, String)
        (OnFailureJobMode, String)
        (IgnoreOnIsolate, bool)
        (StopWhenUnneeded, bool)
        (RefuseManualStart, bool)
        (RefuseManualStop, bool)
        (AllowIsolate, bool)
        (DefaultDependencies, bool)
        (CollectMode, enum { Inactive = "inactive", InactiveOrFailed = "inactive-or-failed" })
        (FailureAction, enum { None = "none", Reboot = "reboot", RebootForce = "reboot-force", RebootImmediate = "reboot-immediate", PowerOff = "poweroff", PowerOffForce = "poweroff-force", PowerOffImmediate = "poweroff-immediate", Exit = "exit", ExitForce = "exit-force" })
        (SuccessAction, enum { None = "none", Reboot = "reboot", RebootForce = "reboot-force", RebootImmediate = "reboot-immediate", PowerOff = "poweroff", PowerOffForce = "poweroff-force", PowerOffImmediate = "poweroff-immediate", Exit = "exit", ExitForce = "exit-force" })
        (FailureActionExitStatus, String)
        (SuccessActionExitStatus, String)
        (JobTimeoutSec, String)
        (JobRunningTimeoutSec, String)
        (JobTimeoutAction, String)
        (JobTimeoutRebootArgument, String)
        (StartLimitIntervalSec, String)
        (StartLimitBurst, String)
        (StartLimitAction, String)
        (RebootArgument, String)
        (SourcePath, String)

        // TODO: Condition*
    ]
}

directives! {
    Install [
        (Alias, String)
        (WantedBy, multiple String)
        (RequiredBy, multiple String)
        (Also, multiple String)
    ]
}

directives! {
    Service [
        // TODO: Should be named 'Type'
        (ServiceType = "Type", enum { Simple = "simple", Exec = "exec", Forking = "forking", OneShot = "oneshot", DBus = "dbus", Notify = "notify", Idle = "idle" })
        (RemainAfterExit, bool)
        (GuessMainPID, bool)
        (PIDFile, String)
        (BusName, String)
        (ExecStart, multiple String)
        (ExecStartPre, multiple String)
        (ExecStartPost, multiple String)
        (ExecCondition, String)
        (ExecReload, multiple String)
        (ExecStop, multiple String)
        (ExecStopPost, String)
        (Environment, multiple String)
        (RestartSec, String)
        (TimeoutStartSec, String)
        (TimeoutStopSec, String)
        (TimeoutAbortSec, String)
        (TimeoutSec, String)
        (TimeoutStartFailureMode, String)
        (TimeoutStopFailureMode, String)
        (RuntimeMaxSec, String)
        (WatchdogSec, String)
        (Restart, enum { No = "no", OnSuccess = "on-success", OnFailure = "on-failure", OnAbnormal = "on-abnormal", OnWatchdog = "on-watchdog", OnAbort = "on-abort", Always = "always"})

        // TODO: The rest
    ]
}

directives! {
    ResourceControl [
        (CPUAccounting = "CpuAccounting", String)
        (CPUWeight, String)
        (StartupCPUWeight, String)
        (CPUQuota, String)
        (CPUQuotaPeriodSec, String)
        (AllowedCPUs, String)
        (AllowedMemoryNodes, String)
        (MemoryAccounting, String)
        (MemoryMin, String)
        (MemoryLow, String)
        (MemoryHigh, String)
        (MemoryMax, String)
        (MemorySwapMax, String)
        (TasksAccounting, bool)
        (TasksMax, String)
        (IOAccounting, bool)
        (IOWeight, String)
        (StartupIOWeight, String)
        (IODeviceWeight, String)
        (IOReadBandwidthMax, String)
        (IOWriteBandwidthMax, String)
        (IOReadIOPSMax, String)
        (IOWriteIOPSMax, String)
        (IODeviceLatencyTargetSec, String)
        (IPAccounting, String)
        (IpAddressAllow = "IPAddressAllow", String)
        (IpAddressDeny = "IPAddressDeny", String)
        (IPIngressFilterPath, String)
        (IPEgressFilterPath, String)
        (DeviceAllow, multiple String)
        (DevicePolicy, enum { Auto = "auto", Closed = "closed", Strict = "strict" })
        (Slice, String)
        (Delegate, String)
        (DisableControllers, String)
        (ManagedOOMSwap, String)
        (ManagedOOMMemoryPressure, String)
        (ManagedOOMMemoryPressureLimit, String)
        (ManagedOOMPreference, String)
    ]
}

directives! {
    Timer [
        (OnActiveSec, String)
        (OnBootSec, String)
        (OnStartupSec, String)
        (OnUnitActiveSec, String)
        (OnUnitInactiveSec, String)
        (OnCalendar, multiple String)
        (AccuracySec, String)
        (RandomizedDelaySec, convert Duration => u64; e => e.as_secs())
        (FixedRandomDelay, bool)
        (OnClockChange, String)
        (OnTimezoneChange, String)
        (Unit, String)
        (Persistent, String)
        (WakeSystem, String)
        (RemainAfterElapse, String)
    ]
}
