use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use egui::Vec2;
use rog_aura::{usb::AuraPowerDev, AuraEffect, AuraModeNum};
use rog_profiles::{fan_curve_set::FanCurveSet, FanCurvePU, Profile};
use rog_supported::SupportedFunctions;

use crate::{error::Result, RogDbusClientBlocking};

#[derive(Clone, Debug)]
pub struct BiosState {
    /// To be shared to a thread that checks notifications.
    /// It's a bit general in that it won't provide *what* was
    /// updated, so the full state needs refresh
    pub was_notified: Arc<AtomicBool>,
    pub post_sound: bool,
    pub dedicated_gfx: bool,
    pub panel_overdrive: bool,
    pub dgpu_disable: bool,
    pub egpu_enable: bool,
}

impl BiosState {
    pub fn new(
        was_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        Ok(Self {
            was_notified,
            post_sound: if supported.rog_bios_ctrl.post_sound {
                dbus.proxies().rog_bios().post_boot_sound()? != 0
            } else {
                false
            },
            dedicated_gfx: if supported.rog_bios_ctrl.dedicated_gfx {
                dbus.proxies().rog_bios().dedicated_graphic_mode()? != 0
            } else {
                false
            },
            panel_overdrive: if supported.rog_bios_ctrl.panel_overdrive {
                dbus.proxies().rog_bios().panel_overdrive()? != 0
            } else {
                false
            },
            // TODO: needs supergfx
            dgpu_disable: supported.rog_bios_ctrl.dgpu_disable,
            egpu_enable: supported.rog_bios_ctrl.egpu_enable,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ProfilesState {
    pub was_notified: Arc<AtomicBool>,
    pub list: Vec<Profile>,
    pub current: Profile,
}

impl ProfilesState {
    pub fn new(
        was_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        Ok(Self {
            was_notified,
            list: if supported.platform_profile.platform_profile {
                dbus.proxies().profile().profiles()?
            } else {
                vec![]
            },
            current: if supported.platform_profile.platform_profile {
                dbus.proxies().profile().active_profile()?
            } else {
                Profile::Balanced
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct FanCurvesState {
    pub was_notified: Arc<AtomicBool>,
    pub show_curve: Profile,
    pub show_graph: FanCurvePU,
    pub enabled: HashSet<Profile>,
    pub curves: HashMap<Profile, FanCurveSet>,
    pub drag_delta: Vec2,
}

impl FanCurvesState {
    pub fn new(
        was_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        let profiles = if supported.platform_profile.platform_profile {
            dbus.proxies().profile().profiles()?
        } else {
            vec![Profile::Balanced, Profile::Quiet, Profile::Performance]
        };
        let enabled = if supported.platform_profile.fan_curves {
            HashSet::from_iter(
                dbus.proxies()
                    .profile()
                    .enabled_fan_profiles()?
                    .iter()
                    .cloned(),
            )
        } else {
            HashSet::from([Profile::Balanced, Profile::Quiet, Profile::Performance])
        };

        let mut curves: HashMap<Profile, FanCurveSet> = HashMap::new();
        profiles.iter().for_each(|p| {
            if supported.platform_profile.fan_curves {
                if let Ok(curve) = dbus.proxies().profile().fan_curve_data(*p) {
                    curves.insert(*p, curve);
                }
            } else {
                let mut curve = FanCurveSet::default();
                curve.cpu.pwm = [30, 40, 60, 100, 140, 180, 200, 250];
                curve.cpu.temp = [20, 30, 40, 50, 70, 80, 90, 100];
                curve.gpu.pwm = [40, 80, 100, 140, 170, 200, 230, 250];
                curve.gpu.temp = [20, 30, 40, 50, 70, 80, 90, 100];
                curves.insert(*p, curve);
            }
        });

        let show_curve = if supported.platform_profile.fan_curves {
            dbus.proxies().profile().active_profile()?
        } else {
            Profile::Balanced
        };

        Ok(Self {
            was_notified,
            show_curve,
            show_graph: FanCurvePU::CPU,
            enabled,
            curves,
            drag_delta: Vec2::default(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct AuraState {
    pub was_notified: Arc<AtomicBool>,
    pub current_mode: AuraModeNum,
    pub modes: BTreeMap<AuraModeNum, AuraEffect>,
    pub enabled: AuraPowerDev,
    /// Brightness from 0-3
    pub bright: i16,
}

impl AuraState {
    pub fn new(
        was_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        Ok(Self {
            was_notified,
            current_mode: if !supported.keyboard_led.stock_led_modes.is_empty() {
                dbus.proxies().led().led_mode()?
            } else {
                AuraModeNum::Static
            },

            modes: if !supported.keyboard_led.stock_led_modes.is_empty() {
                dbus.proxies().led().led_modes()?
            } else {
                BTreeMap::new()
            },
            enabled: dbus.proxies().led().leds_enabled()?,
            bright: if !supported.keyboard_led.brightness_set {
                dbus.proxies().led().led_brightness()?
            } else {
                2
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct AnimeState {
    pub was_notified: Arc<AtomicBool>,
    pub bright: u8,
    pub boot: bool,
    pub awake: bool,
    pub sleep: bool,
}

impl AnimeState {
    pub fn new(
        was_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        Ok(Self {
            was_notified,
            boot: if supported.anime_ctrl.0 {
                dbus.proxies().anime().boot_enabled()?
            } else {
                false
            },
            awake: if supported.anime_ctrl.0 {
                dbus.proxies().anime().awake_enabled()?
            } else {
                false
            },
            // TODO:
            sleep: false,
            bright: 200,
        })
    }
}

#[derive(Debug)]
pub struct PageDataStates {
    pub notifs_enabled: Arc<AtomicBool>,
    pub was_notified: Arc<AtomicBool>,
    /// Because much of the app state here is the same as `RogBiosSupportedFunctions`
    /// we can re-use that structure.
    pub bios: BiosState,
    pub aura: AuraState,
    pub anime: AnimeState,
    pub profiles: ProfilesState,
    pub fan_curves: FanCurvesState,
    pub charge_limit: i16,
    pub error: Option<String>,
}

impl PageDataStates {
    pub fn new(
        notifs_enabled: Arc<AtomicBool>,
        charge_notified: Arc<AtomicBool>,
        bios_notified: Arc<AtomicBool>,
        aura_notified: Arc<AtomicBool>,
        anime_notified: Arc<AtomicBool>,
        profiles_notified: Arc<AtomicBool>,
        fans_notified: Arc<AtomicBool>,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<Self> {
        Ok(Self {
            notifs_enabled,
            was_notified: charge_notified,
            charge_limit: dbus.proxies().charge().limit()?,
            bios: BiosState::new(bios_notified, supported, dbus)?,
            aura: AuraState::new(aura_notified, supported, dbus)?,
            anime: AnimeState::new(anime_notified, supported, dbus)?,
            profiles: ProfilesState::new(profiles_notified, supported, dbus)?,
            fan_curves: FanCurvesState::new(fans_notified, supported, dbus)?,
            error: None,
        })
    }

    pub fn refresh_if_notfied(
        &mut self,
        supported: &SupportedFunctions,
        dbus: &RogDbusClientBlocking,
    ) -> Result<bool> {
        let mut notified = false;
        if self.was_notified.load(Ordering::SeqCst) {
            self.charge_limit = dbus.proxies().charge().limit()?;
            self.was_notified.store(false, Ordering::SeqCst);
            notified = true;
        }

        if self.aura.was_notified.load(Ordering::SeqCst) {
            self.aura = AuraState::new(self.aura.was_notified.clone(), supported, dbus)?;
            self.aura.was_notified.store(false, Ordering::SeqCst);
            notified = true;
        }

        if self.bios.was_notified.load(Ordering::SeqCst) {
            self.bios = BiosState::new(self.bios.was_notified.clone(), supported, dbus)?;
            self.bios.was_notified.store(false, Ordering::SeqCst);
            notified = true;
        }

        if self.profiles.was_notified.load(Ordering::SeqCst) {
            self.profiles =
                ProfilesState::new(self.profiles.was_notified.clone(), supported, dbus)?;
            self.profiles.was_notified.store(false, Ordering::SeqCst);
            notified = true;
        }

        if self.fan_curves.was_notified.load(Ordering::SeqCst) {
            self.fan_curves =
                FanCurvesState::new(self.fan_curves.was_notified.clone(), supported, dbus)?;
            self.fan_curves.was_notified.store(false, Ordering::SeqCst);
            notified = true;
        }
        Ok(notified)
    }
}

impl Default for PageDataStates {
    fn default() -> Self {
        Self {
            notifs_enabled: Default::default(),
            was_notified: Default::default(),
            bios: BiosState {
                was_notified: Default::default(),
                post_sound: Default::default(),
                dedicated_gfx: Default::default(),
                panel_overdrive: Default::default(),
                dgpu_disable: Default::default(),
                egpu_enable: Default::default(),
            },
            aura: AuraState {
                was_notified: Default::default(),
                current_mode: AuraModeNum::Static,
                modes: Default::default(),
                enabled: AuraPowerDev {
                    x1866: vec![],
                    x19b6: vec![],
                },
                bright: Default::default(),
            },
            anime: AnimeState {
                was_notified: Default::default(),
                bright: Default::default(),
                boot: Default::default(),
                awake: Default::default(),
                sleep: Default::default(),
            },
            profiles: ProfilesState {
                was_notified: Default::default(),
                list: Default::default(),
                current: Default::default(),
            },
            fan_curves: FanCurvesState {
                was_notified: Default::default(),
                show_curve: Default::default(),
                show_graph: Default::default(),
                enabled: Default::default(),
                curves: Default::default(),
                drag_delta: Default::default(),
            },
            charge_limit: Default::default(),
            error: Default::default(),
        }
    }
}
