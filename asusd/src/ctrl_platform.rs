use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;

use config_traits::StdConfig;
use log::{debug, error, info, warn};
use rog_platform::asus_armoury::{AttrValue, FirmwareAttribute, FirmwareAttributes};
use rog_platform::cpu::{CPUControl, CPUGovernor, CPUEPP};
use rog_platform::platform::{Properties, RogPlatform, ThrottlePolicy};
use rog_platform::power::AsusPower;
use zbus::export::futures_util::lock::Mutex;
use zbus::fdo::Error as FdoErr;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Connection};

use crate::config::Config;
use crate::error::RogError;
use crate::{task_watch_item, CtrlTask, ReloadAndNotify};

const PLATFORM_ZBUS_PATH: &str = "/xyz/ljones";

macro_rules! platform_get_value {
    ($self:ident, $property:tt, $prop_name:literal) => {
        concat_idents::concat_idents!(has = has_, $property {
            if $self.platform.has() {
                concat_idents::concat_idents!(get = get_, $property {
                    $self.platform
                    .get()
                    .map_err(|err| {
                        warn!("{}: {}", $prop_name, err);
                        FdoErr::Failed(format!("RogPlatform: {}: {}", $prop_name, err))
                    })
                })
            } else {
                return Err(FdoErr::NotSupported(format!("RogPlatform: {} not supported", $prop_name)));
            }
        })
    }
}

#[derive(Clone)]
pub struct CtrlPlatform {
    power: AsusPower,
    platform: RogPlatform,
    cpu_control: Option<CPUControl>,
    config: Arc<Mutex<Config>>
}

impl CtrlPlatform {
    pub fn new(
        config: Arc<Mutex<Config>>,
        config_path: &Path,
        signal_context: SignalEmitter<'static>
    ) -> Result<Self, RogError> {
        let platform = RogPlatform::new()?;
        let power = AsusPower::new()?;
        let config1 = config.clone();
        let config_path = config_path.to_owned();

        let ret_self = CtrlPlatform {
            power,
            platform,
            config,
            cpu_control: CPUControl::new()
                .map_err(|e| error!("Couldn't get CPU control sysfs: {e}"))
                .ok()
        };
        let mut inotify_self = ret_self.clone();

        tokio::spawn(async move {
            use zbus::export::futures_util::StreamExt;
            info!("Starting inotify watch for asusd config file");

            let mut buffer = [0; 32];
            loop {
                // vi and vim do stupid shit causing the file watch to be removed
                let inotify = inotify::Inotify::init().unwrap();
                inotify
                    .watches()
                    .add(
                        &config_path,
                        inotify::WatchMask::MODIFY
                            | inotify::WatchMask::CLOSE_WRITE
                            | inotify::WatchMask::ATTRIB
                            | inotify::WatchMask::CREATE
                    )
                    .inspect_err(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            error!("Not found: {:?}", config_path);
                        } else {
                            error!("Could not set asusd config inotify: {:?}", config_path);
                        }
                    })
                    .ok();
                let mut events = inotify.into_event_stream(&mut buffer).unwrap();

                while let Some(ev) = events.next().await {
                    if let Ok(ev) = ev {
                        if ev.mask == inotify::EventMask::IGNORED {
                            warn!(
                                "Something modified asusd.ron vi/vim style. Now need to reload \
                                 inotify watch"
                            );
                            break;
                        }
                    }

                    let res = config1.lock().await.read_new();
                    if let Some(new_cfg) = res {
                        inotify_self
                            .reload_and_notify(&signal_context, new_cfg)
                            .await
                            .unwrap();
                    }
                }
            }
        });

        Ok(ret_self)
    }

    async fn restore_charge_limit(&self) {
        let limit = self.config.lock().await.base_charge_control_end_threshold;
        if limit > 0
            && std::mem::replace(
                &mut self.config.lock().await.charge_control_end_threshold,
                limit
            ) != limit
        {
            self.power
                .set_charge_control_end_threshold(limit)
                .map_err(|e| {
                    error!("Couldn't restore charge limit: {e}");
                })
                .ok();
            self.config.lock().await.write();
        }
    }

    async fn run_ac_or_bat_cmd(&self, power_plugged: bool) {
        let prog: Vec<String> = if power_plugged {
            // AC ONLINE
            self.config
                .lock()
                .await
                .ac_command
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        } else {
            // BATTERY
            self.config
                .lock()
                .await
                .bat_command
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };
        if prog.len() > 1 {
            let mut cmd = Command::new(&prog[0]);
            for arg in prog.iter().skip(1) {
                cmd.arg(arg);
            }
            if let Err(e) = cmd.spawn() {
                if power_plugged {
                    error!("AC power command error: {e}");
                } else {
                    error!("Battery power command error: {e}");
                }
            }
        }
    }

    fn check_and_set_epp(&self, enegy_pref: CPUEPP, change_epp: bool) {
        if !change_epp {
            info!("ThrottlePolicy unlinked from EPP");
            return;
        }
        info!("ThrottlePolicy setting EPP");
        if let Some(cpu) = self.cpu_control.as_ref() {
            if let Ok(epp) = cpu.get_available_epp() {
                debug!("Available EPP: {epp:?}");
                if epp.contains(&enegy_pref) {
                    debug!("Setting {enegy_pref:?}");
                    cpu.set_epp(enegy_pref).ok();
                } else if let Ok(gov) = cpu.get_governor() {
                    if gov != CPUGovernor::Powersave {
                        warn!("powersave governor is not is use, trying to set.");
                        cpu.set_governor(CPUGovernor::Powersave)
                            .map_err(|e| error!("couldn't set powersave: {e:?}"))
                            .ok();
                        if epp.contains(&enegy_pref) {
                            debug!("Setting {enegy_pref:?}");
                            cpu.set_epp(enegy_pref)
                                .map_err(|e| error!("couldn't set EPP: {e:?}"))
                                .ok();
                        }
                    }
                }
            }
        }
    }

    async fn get_config_epp_for_throttle(&self, throttle: ThrottlePolicy) -> CPUEPP {
        match throttle {
            ThrottlePolicy::Balanced => self.config.lock().await.throttle_balanced_epp,
            ThrottlePolicy::Performance => self.config.lock().await.throttle_performance_epp,
            ThrottlePolicy::Quiet => self.config.lock().await.throttle_quiet_epp
        }
    }

    async fn update_policy_ac_or_bat(&self, power_plugged: bool, change_epp: bool) {
        if power_plugged && !self.config.lock().await.change_throttle_policy_on_ac {
            debug!(
                "Power status changed but set_throttle_policy_on_ac set false. Not setting the \
                 thing"
            );
            return;
        }
        if !power_plugged && !self.config.lock().await.change_throttle_policy_on_battery {
            debug!(
                "Power status changed but set_throttle_policy_on_battery set false. Not setting \
                 the thing"
            );
            return;
        }

        let throttle = if power_plugged {
            self.config.lock().await.throttle_policy_on_ac
        } else {
            self.config.lock().await.throttle_policy_on_battery
        };
        debug!("Setting {throttle:?} before EPP");
        let epp = self.get_config_epp_for_throttle(throttle).await;
        self.platform
            .set_throttle_thermal_policy(throttle.into())
            .ok();
        self.check_and_set_epp(epp, change_epp);
    }
}

#[interface(name = "xyz.ljones.Platform")]
impl CtrlPlatform {
    #[zbus(property)]
    async fn version(&self) -> String {
        crate::VERSION.to_string()
    }

    /// Returns a list of property names that this system supports
    async fn supported_properties(&self) -> Vec<Properties> {
        let mut supported = Vec::new();

        macro_rules! platform_name {
            ($property:tt, $prop_name:ty) => {
                concat_idents::concat_idents!(has = has_, $property {
                    if self.platform.has() {
                        supported.push($prop_name.to_owned());
                    }
                })
            }
        }

        macro_rules! power_name {
            ($property:tt, $prop_name:ty) => {
                concat_idents::concat_idents!(has = has_, $property {
                    if self.power.has() {
                        supported.push($prop_name.to_owned());
                    }
                })
            }
        }

        // TODO: automate this
        power_name!(
            charge_control_end_threshold,
            Properties::ChargeControlEndThreshold
        );

        platform_name!(throttle_thermal_policy, Properties::ThrottlePolicy);

        supported
    }

    #[zbus(property)]
    fn charge_control_end_threshold(&self) -> Result<u8, FdoErr> {
        let limit = self.power.get_charge_control_end_threshold()?;
        Ok(limit)
    }

    #[zbus(property)]
    async fn set_charge_control_end_threshold(&mut self, limit: u8) -> Result<(), FdoErr> {
        if !(20..=100).contains(&limit) {
            return Err(RogError::ChargeLimit(limit))?;
        }
        self.power.set_charge_control_end_threshold(limit)?;
        self.config.lock().await.charge_control_end_threshold = limit;
        self.config.lock().await.base_charge_control_end_threshold = limit;
        self.config.lock().await.write();
        Ok(())
    }

    async fn one_shot_full_charge(&self) -> Result<(), FdoErr> {
        let base_limit = std::mem::replace(
            &mut self.config.lock().await.charge_control_end_threshold,
            100
        );
        if base_limit != 100 {
            self.power.set_charge_control_end_threshold(100)?;
            self.config.lock().await.base_charge_control_end_threshold = base_limit;
            self.config.lock().await.write();
        }
        Ok(())
    }

    /// Toggle to next platform_profile. Names provided by `Profiles`.
    /// If fan-curves are supported will also activate a fan curve for profile.
    async fn next_throttle_thermal_policy(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>
    ) -> Result<(), FdoErr> {
        let policy: ThrottlePolicy =
            platform_get_value!(self, throttle_thermal_policy, "throttle_thermal_policy")
                .map(|n| n.into())?;
        let policy = ThrottlePolicy::next(policy);

        if self.platform.has_throttle_thermal_policy() {
            let change_epp = self.config.lock().await.throttle_policy_linked_epp;
            let epp = self.get_config_epp_for_throttle(policy).await;
            self.check_and_set_epp(epp, change_epp);
            self.platform
                .set_throttle_thermal_policy(policy.into())
                .map_err(|err| {
                    warn!("throttle_thermal_policy {}", err);
                    FdoErr::Failed(format!("RogPlatform: throttle_thermal_policy: {err}"))
                })?;
            Ok(self.throttle_thermal_policy_changed(&ctxt).await?)
        } else {
            Err(FdoErr::NotSupported(
                "RogPlatform: throttle_thermal_policy not supported".to_owned()
            ))
        }
    }

    #[zbus(property)]
    fn throttle_thermal_policy(&self) -> Result<ThrottlePolicy, FdoErr> {
        platform_get_value!(self, throttle_thermal_policy, "throttle_thermal_policy")
            .map(|n| n.into())
    }

    #[zbus(property)]
    async fn set_throttle_thermal_policy(&mut self, policy: ThrottlePolicy) -> Result<(), FdoErr> {
        // TODO: watch for external changes
        if self.platform.has_throttle_thermal_policy() {
            let change_epp = self.config.lock().await.throttle_policy_linked_epp;
            let epp = self.get_config_epp_for_throttle(policy).await;
            self.check_and_set_epp(epp, change_epp);
            self.config.lock().await.write();
            self.platform
                .set_throttle_thermal_policy(policy.into())
                .map_err(|err| {
                    warn!("throttle_thermal_policy {}", err);
                    FdoErr::Failed(format!("RogPlatform: throttle_thermal_policy: {err}"))
                })
        } else {
            Err(FdoErr::NotSupported(
                "RogPlatform: throttle_thermal_policy not supported".to_owned()
            ))
        }
    }

    #[zbus(property)]
    async fn throttle_policy_linked_epp(&self) -> Result<bool, FdoErr> {
        Ok(self.config.lock().await.throttle_policy_linked_epp)
    }

    #[zbus(property)]
    async fn set_throttle_policy_linked_epp(&self, linked: bool) -> Result<(), zbus::Error> {
        self.config.lock().await.throttle_policy_linked_epp = linked;
        self.config.lock().await.write();
        Ok(())
    }

    #[zbus(property)]
    async fn throttle_policy_on_battery(&self) -> Result<ThrottlePolicy, FdoErr> {
        Ok(self.config.lock().await.throttle_policy_on_battery)
    }

    #[zbus(property)]
    async fn set_throttle_policy_on_battery(
        &mut self,
        policy: ThrottlePolicy
    ) -> Result<(), FdoErr> {
        self.config.lock().await.throttle_policy_on_battery = policy;
        self.set_throttle_thermal_policy(policy).await?;
        self.config.lock().await.write();
        Ok(())
    }

    #[zbus(property)]
    async fn change_throttle_policy_on_battery(&self) -> Result<bool, FdoErr> {
        Ok(self.config.lock().await.change_throttle_policy_on_battery)
    }

    #[zbus(property)]
    async fn set_change_throttle_policy_on_battery(&mut self, change: bool) -> Result<(), FdoErr> {
        self.config.lock().await.change_throttle_policy_on_battery = change;
        self.config.lock().await.write();
        Ok(())
    }

    #[zbus(property)]
    async fn throttle_policy_on_ac(&self) -> Result<ThrottlePolicy, FdoErr> {
        Ok(self.config.lock().await.throttle_policy_on_ac)
    }

    #[zbus(property)]
    async fn set_throttle_policy_on_ac(&mut self, policy: ThrottlePolicy) -> Result<(), FdoErr> {
        self.config.lock().await.throttle_policy_on_ac = policy;
        self.set_throttle_thermal_policy(policy).await?;
        self.config.lock().await.write();
        Ok(())
    }

    #[zbus(property)]
    async fn change_throttle_policy_on_ac(&self) -> Result<bool, FdoErr> {
        Ok(self.config.lock().await.change_throttle_policy_on_ac)
    }

    #[zbus(property)]
    async fn set_change_throttle_policy_on_ac(&mut self, change: bool) -> Result<(), FdoErr> {
        self.config.lock().await.change_throttle_policy_on_ac = change;
        self.config.lock().await.write();
        Ok(())
    }

    /// The energy_performance_preference for the quiet throttle/platform
    /// profile
    #[zbus(property)]
    async fn throttle_quiet_epp(&self) -> Result<CPUEPP, FdoErr> {
        Ok(self.config.lock().await.throttle_quiet_epp)
    }

    #[zbus(property)]
    async fn set_throttle_quiet_epp(&mut self, epp: CPUEPP) -> Result<(), FdoErr> {
        let change_pp = self.config.lock().await.throttle_policy_linked_epp;
        self.config.lock().await.throttle_quiet_epp = epp;
        self.check_and_set_epp(epp, change_pp);
        self.config.lock().await.write();
        Ok(())
    }

    /// The energy_performance_preference for the balanced throttle/platform
    /// profile
    #[zbus(property)]
    async fn throttle_balanced_epp(&self) -> Result<CPUEPP, FdoErr> {
        Ok(self.config.lock().await.throttle_balanced_epp)
    }

    #[zbus(property)]
    async fn set_throttle_balanced_epp(&mut self, epp: CPUEPP) -> Result<(), FdoErr> {
        let change_pp = self.config.lock().await.throttle_policy_linked_epp;
        self.config.lock().await.throttle_balanced_epp = epp;
        self.check_and_set_epp(epp, change_pp);
        self.config.lock().await.write();
        Ok(())
    }

    /// The energy_performance_preference for the performance throttle/platform
    /// profile
    #[zbus(property)]
    async fn throttle_performance_epp(&self) -> Result<CPUEPP, FdoErr> {
        Ok(self.config.lock().await.throttle_performance_epp)
    }

    #[zbus(property)]
    async fn set_throttle_performance_epp(&mut self, epp: CPUEPP) -> Result<(), FdoErr> {
        let change_pp = self.config.lock().await.throttle_policy_linked_epp;
        self.config.lock().await.throttle_performance_epp = epp;
        self.check_and_set_epp(epp, change_pp);
        self.config.lock().await.write();
        Ok(())
    }
}

impl crate::ZbusRun for CtrlPlatform {
    async fn add_to_server(self, server: &mut Connection) {
        Self::add_to_server_helper(self, PLATFORM_ZBUS_PATH, server).await;
    }
}

impl ReloadAndNotify for CtrlPlatform {
    type Data = Config;

    /// Called on config file changed externally
    async fn reload_and_notify(
        &mut self,
        signal_context: &SignalEmitter<'static>,
        data: Self::Data
    ) -> Result<(), RogError> {
        let mut config = self.config.lock().await;
        if *config != data {
            info!("asusd.ron updated externally, reloading and updating internal copy");

            let mut base_charge_control_end_threshold = None;

            if self.power.has_charge_control_end_threshold()
                && data.charge_control_end_threshold != config.charge_control_end_threshold
            {
                let limit = data.charge_control_end_threshold;
                warn!("setting charge_control_end_threshold to {limit}");
                self.power.set_charge_control_end_threshold(limit)?;
                self.charge_control_end_threshold_changed(signal_context)
                    .await?;
                base_charge_control_end_threshold = (config.base_charge_control_end_threshold > 0)
                    .then_some(config.base_charge_control_end_threshold)
                    .or(Some(limit));
            }

            if self.platform.has_throttle_thermal_policy()
                && config.throttle_policy_linked_epp != data.throttle_policy_linked_epp
            {
                let profile: ThrottlePolicy =
                    ThrottlePolicy::from_str(self.platform.get_platform_profile()?.as_str())?;

                let epp = match profile {
                    ThrottlePolicy::Balanced => data.throttle_balanced_epp,
                    ThrottlePolicy::Performance => data.throttle_performance_epp,
                    ThrottlePolicy::Quiet => data.throttle_quiet_epp
                };
                warn!("setting epp to {epp:?}");
                self.check_and_set_epp(epp, true);
            }
            // reload_and_notify!(throttle_thermal_policy, "throttle_thermal_policy");

            *config = data;
            config.base_charge_control_end_threshold =
                base_charge_control_end_threshold.unwrap_or_default();
        }
        Ok(())
    }
}

impl crate::Reloadable for CtrlPlatform {
    async fn reload(&mut self) -> Result<(), RogError> {
        info!("Begin Platform settings restore");
        if self.power.has_charge_control_end_threshold() {
            // self.restore_charge_limit().await;
            let limit = self.config.lock().await.charge_control_end_threshold;
            info!("reloading charge_control_end_threshold to {limit}");
            self.power.set_charge_control_end_threshold(limit)?;
        } else {
            warn!("No charge_control_end_threshold found")
        }

        if let Ok(power_plugged) = self.power.get_online() {
            self.config.lock().await.last_power_plugged = power_plugged;
            if self.platform.has_throttle_thermal_policy() {
                let change_epp = self.config.lock().await.throttle_policy_linked_epp;
                self.update_policy_ac_or_bat(power_plugged > 0, change_epp)
                    .await;
            }
            self.run_ac_or_bat_cmd(power_plugged > 0).await;
        }

        Ok(())
    }
}

impl CtrlPlatform {
    task_watch_item!(charge_control_end_threshold "charge_control_end_threshold" power);
}

impl CtrlTask for CtrlPlatform {
    fn zbus_path() -> &'static str {
        PLATFORM_ZBUS_PATH
    }

    async fn create_tasks(&self, signal_ctxt: SignalEmitter<'static>) -> Result<(), RogError> {
        let platform1 = self.clone();
        let platform2 = self.clone();
        let platform3 = self.clone();
        self.create_sys_event_tasks(
            move |sleeping| {
                let platform1 = platform1.clone();
                async move {
                    // This block is commented out due to some kind of issue reported. Maybe the
                    // desktops used were storing a value whcih was then read here.
                    // Don't store it on suspend, assume that the current config setting is desired
                    // if sleeping && platform1.power.has_charge_control_end_threshold() {
                    //     platform1.config.lock().await.charge_control_end_threshold = platform1
                    //         .power
                    //         .get_charge_control_end_threshold()
                    //         .unwrap_or(100);
                    // } else
                    if !sleeping && platform1.power.has_charge_control_end_threshold() {
                        platform1
                            .power
                            .set_charge_control_end_threshold(
                                platform1.config.lock().await.charge_control_end_threshold
                            )
                            .ok();
                    }
                    if let Ok(power_plugged) = platform1.power.get_online() {
                        if platform1.config.lock().await.last_power_plugged != power_plugged {
                            if !sleeping && platform1.platform.has_throttle_thermal_policy() {
                                let change_epp =
                                    platform1.config.lock().await.throttle_policy_linked_epp;
                                platform1
                                    .update_policy_ac_or_bat(power_plugged > 0, change_epp)
                                    .await;
                            }
                            if !sleeping {
                                platform1.run_ac_or_bat_cmd(power_plugged > 0).await;
                            }
                            platform1.config.lock().await.last_power_plugged = power_plugged;
                        }
                    }
                }
            },
            move |shutting_down| {
                let platform2 = platform2.clone();
                async move {
                    info!("RogPlatform reloading panel_od");
                    let lock = platform2.config.lock().await;
                    if shutting_down
                        && platform2.power.has_charge_control_end_threshold()
                        && lock.base_charge_control_end_threshold > 0
                    {
                        info!("RogPlatform restoring charge_control_end_threshold");
                        platform2
                            .power
                            .set_charge_control_end_threshold(
                                lock.base_charge_control_end_threshold
                            )
                            .map_err(|err| {
                                warn!("CtrlCharge: charge_control_end_threshold {}", err);
                                err
                            })
                            .ok();
                    }
                }
            },
            move |_lid_closed| {
                // on lid change
                async move {}
            },
            move |power_plugged| {
                let platform3 = platform3.clone();
                // power change
                async move {
                    if platform3.platform.has_throttle_thermal_policy() {
                        let change_epp = platform3.config.lock().await.throttle_policy_linked_epp;
                        platform3
                            .update_policy_ac_or_bat(power_plugged, change_epp)
                            .await;
                    }
                    platform3.run_ac_or_bat_cmd(power_plugged).await;
                    // In case one-shot charge was used, restore the old charge limit
                    if platform3.power.has_charge_control_end_threshold() && !power_plugged {
                        platform3.restore_charge_limit().await;
                    }
                }
            }
        )
        .await;

        // This spawns a new task for every item.
        // TODO: find a better way to manage this
        self.watch_charge_control_end_threshold(signal_ctxt.clone())
            .await?;

        let watch_throttle_thermal_policy = self.platform.monitor_throttle_thermal_policy()?;
        let ctrl = self.clone();

        // Need a copy here, not ideal. But first use in asus_armoury.rs is
        // moved to zbus
        let attrs = FirmwareAttributes::new();
        tokio::spawn(async move {
            use futures_lite::StreamExt;
            let mut buffer = [0; 32];
            if let Ok(mut stream) = watch_throttle_thermal_policy.into_event_stream(&mut buffer) {
                while (stream.next().await).is_some() {
                    // this blocks
                    debug!("Platform: watch_throttle_thermal_policy changed");
                    if let Ok(profile) = ctrl
                        .platform
                        .get_throttle_thermal_policy()
                        .map(ThrottlePolicy::from)
                        .map_err(|e| {
                            error!("Platform: get_throttle_thermal_policy error: {e}");
                        })
                    {
                        let change_epp = ctrl.config.lock().await.throttle_policy_linked_epp;
                        let epp = ctrl.get_config_epp_for_throttle(profile).await;
                        ctrl.check_and_set_epp(epp, change_epp);
                        ctrl.throttle_thermal_policy_changed(&signal_ctxt)
                            .await
                            .ok();
                        for attr in attrs.attributes().iter() {
                            let name: FirmwareAttribute = attr.name().into();
                            if name.is_ppt() {
                                let mut do_default = false;
                                if let Some(tunings) =
                                    ctrl.config.lock().await.profile_tunings.get(&profile)
                                {
                                    if let Some(tune) = tunings.get(&name) {
                                        attr.set_current_value(AttrValue::Integer(*tune))
                                            .map_err(|e| {
                                                error!("Failed to set {}: {e}", <&str>::from(name));
                                            })
                                            .ok();
                                    } else {
                                        do_default = true;
                                    }
                                } else {
                                    do_default = true;
                                }
                                if do_default {
                                    let default = attr.default_value().clone();
                                    attr.set_current_value(default)
                                        .map_err(|e| {
                                            error!("Failed to set {}: {e}", <&str>::from(name));
                                        })
                                        .ok();
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
