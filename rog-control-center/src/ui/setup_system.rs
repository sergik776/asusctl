use std::sync::{Arc, Mutex};

use concat_idents::concat_idents;
use log::{debug, error};
use rog_dbus::asus_armoury::AsusArmouryProxy;
use rog_dbus::zbus_platform::{PlatformProxy, PlatformProxyBlocking};
use rog_platform::asus_armoury::FirmwareAttribute;
use rog_platform::platform::Properties;
use slint::ComponentHandle;

use super::show_toast;
use crate::config::Config;
use crate::zbus_proxies::find_iface_async;
use crate::{set_ui_callbacks, set_ui_props_async, AttrMinMax, MainWindow, SystemPageData};

const MINMAX: AttrMinMax = AttrMinMax {
    min: 0,
    max: 0,
    val: -1.0
};

pub fn setup_system_page(ui: &MainWindow, _config: Arc<Mutex<Config>>) {
    let conn = zbus::blocking::Connection::system().unwrap();
    let platform = PlatformProxyBlocking::new(&conn).unwrap();
    // let armoury_attrs =
    // find_iface::<AsusArmouryProxyBlocking>("xyz.ljones.AsusArmoury").unwrap();

    // Null everything before the setup step
    ui.global::<SystemPageData>()
        .set_charge_control_end_threshold(-1.0);
    ui.global::<SystemPageData>().set_platform_profile(-1);
    ui.global::<SystemPageData>().set_panel_overdrive(-1);
    ui.global::<SystemPageData>().set_boot_sound(-1);
    ui.global::<SystemPageData>().set_mini_led_mode(-1);
    ui.global::<SystemPageData>().set_ppt_pl1_spl(MINMAX);
    ui.global::<SystemPageData>().set_ppt_pl2_sppt(MINMAX);
    ui.global::<SystemPageData>().set_ppt_pl3_fppt(MINMAX);
    ui.global::<SystemPageData>().set_ppt_fppt(MINMAX);
    ui.global::<SystemPageData>().set_ppt_apu_sppt(MINMAX);
    ui.global::<SystemPageData>().set_ppt_platform_sppt(MINMAX);
    ui.global::<SystemPageData>().set_nv_dynamic_boost(MINMAX);
    ui.global::<SystemPageData>().set_nv_temp_target(MINMAX);

    let sys_props = platform.supported_properties().unwrap();
    log::debug!("Available system properties: {sys_props:?}");
    if sys_props.contains(&Properties::ChargeControlEndThreshold) {
        ui.global::<SystemPageData>()
            .set_charge_control_end_threshold(60.0);
    }
}

macro_rules! convert_value {
    (bool, $value:expr) => {
        $value == 1
    };
    (i32, $value:expr) => {
        $value as i32
    };
    (f32, $value:expr) => {
        $value as f32
    };
}

macro_rules! convert_to_dbus {
    (bool, $value:expr) => {
        if $value {
            1
        } else {
            0
        }
    };
    (i32, $value:expr) => {
        $value as i32
    };
    (f32, $value:expr) => {
        $value as i32
    };
}

macro_rules! init_property {
    ($property:ident, $handle:expr, $value:expr, $type:tt) => {{
        concat_idents!(setter = set_, $property {
            $handle.global::<SystemPageData>().setter(convert_value!($type, $value));
        });
    }};
}

// For initial setup of min/max/val values
macro_rules! init_minmax_property {
    ($property:ident, $handle:expr, $attr:expr) => {
        let proxy_copy = $attr.clone();
        let handle_copy = $handle.as_weak();
        tokio::spawn(async move {
            let min = proxy_copy.min_value().await.unwrap();
            let max = proxy_copy.max_value().await.unwrap();
            let val = proxy_copy.current_value().await.unwrap() as f32;
            handle_copy
                .upgrade_in_event_loop(move |handle| {
                    concat_idents!(setter = set_, $property {
                        handle
                            .global::<SystemPageData>()
                            .setter(AttrMinMax { min, max, val });
                    });
                })
                .ok();
        });
    };
}

// For handling callbacks from UI value changes
macro_rules! setup_callback {
    ($property:ident, $handle:expr, $attr:expr, $type:tt) => {
        let handle_copy = $handle.as_weak();
        let proxy_copy = $attr.clone();
        concat_idents!(on_callback = on_cb_, $property {
            $handle
                .global::<SystemPageData>()
                .on_callback(move |v| {
                    let handle_copy = handle_copy.clone();
                    let proxy_copy = proxy_copy.clone();
                    tokio::spawn(async move {
                        show_toast(
                            format!("{} successfully set to {}", stringify!($property), v).into(),
                            format!("Setting {} failed", stringify!($property)).into(),
                            handle_copy,
                            proxy_copy.set_current_value(convert_to_dbus!($type, v)).await,
                        );
                    });
                });
        });
    };
}

// For handling callbacks from UI value changes
macro_rules! setup_callback_restore_default {
    ($property:ident, $handle:expr, $attr:expr) => {
        let proxy_copy = $attr.clone();
        concat_idents!(on_callback = on_cb_default_, $property {
            $handle
                .global::<SystemPageData>()
                .on_callback(move || {
                    let proxy_copy = proxy_copy.clone();
                    tokio::spawn(async move {
                        proxy_copy.restore_default().await.ok();
                    });
                });
        });
    };
}

macro_rules! setup_external {
    ($property:ident, $type:tt, $handle:expr, $attr:expr, $value:expr) => {{
        // EXTERNAL CHANGES
        let handle_copy = $handle.as_weak();
        let proxy_copy = $attr.clone();
        concat_idents!(setter = set_, $property {
            tokio::spawn(async move {
                let mut x = proxy_copy.receive_current_value_changed().await;
                use zbus::export::futures_util::StreamExt;
                while let Some(e) = x.next().await {
                    if let Ok(out) = e.get().await {
                        handle_copy
                            .upgrade_in_event_loop(move |handle| {
                                handle
                                    .global::<SystemPageData>()
                                    .setter(convert_value!($type, out));
                            })
                            .ok();
                    }
                }
            });
        });
    }};
}

// For handling external value changes
macro_rules! setup_minmax_external {
    ($property:ident, $handle:expr, $attr:expr, $platform:expr) => {
        let handle_copy = $handle.as_weak();
        let proxy_copy = $attr.clone();
        tokio::spawn(async move {
            let mut x = proxy_copy.receive_current_value_changed().await;
            use zbus::export::futures_util::StreamExt;
            while let Some(e) = x.next().await {
                if let Ok(out) = e.get().await {
                    concat_idents!(getter = get_, $property {
                    handle_copy
                        .upgrade_in_event_loop(move |handle| {
                            let mut tmp: AttrMinMax =
                                handle.global::<SystemPageData>().getter();
                            tmp.val = out as f32;
                            concat_idents!(setter = set_, $property {
                                handle.global::<SystemPageData>().setter(tmp);
                            });
                        })
                        .ok();
                    });
                }
            }
        });

        let handle_copy = $handle.as_weak();
        let proxy_copy = $attr.clone();
        let platform_proxy_copy = $platform.clone();
        tokio::spawn(async move {
            let mut x = platform_proxy_copy.receive_platform_profile_changed().await;
            use zbus::export::futures_util::StreamExt;
            while let Some(e) = x.next().await {
                if let Ok(_) = e.get().await {
                    debug!("receive_platform_profile_changed, getting new {}", stringify!(attr));
                    let min = proxy_copy.min_value().await.unwrap();
                    let max = proxy_copy.max_value().await.unwrap();
                    let val = proxy_copy.current_value().await.unwrap() as f32;
                    handle_copy
                        .upgrade_in_event_loop(move |handle| {
                            concat_idents!(setter = set_, $property {
                                handle
                                    .global::<SystemPageData>()
                                    .setter(AttrMinMax { min, max, val });
                            });
                        })
                        .ok();
                }
            }
        });
    };
}

// This macro expects are consistent naming between proxy calls and slint
// globals
#[macro_export]
macro_rules! set_ui_props_async {
    ($ui:ident, $proxy:ident, $global:ident, $proxy_fn:ident) => {
        if let Ok(value) = $proxy.$proxy_fn().await {
            $ui.upgrade_in_event_loop(move |handle| {
                concat_idents::concat_idents!(set = set_, $proxy_fn {
                    handle.global::<$global>().set(value.into());
                });
            }).ok();
        }
    };
}

pub fn setup_system_page_callbacks(ui: &MainWindow, _states: Arc<Mutex<Config>>) {
    // This tokio spawn exists only to prevent blocking the UI, and to enable use of
    // async zbus interfaces
    let handle = ui.as_weak();

    tokio::spawn(async move {
        // Create the connections/proxies here to prevent future delays in process
        let conn = zbus::Connection::system().await.unwrap();
        let platform = PlatformProxy::new(&conn).await.unwrap();

        set_ui_props_async!(
            handle,
            platform,
            SystemPageData,
            charge_control_end_threshold
        );

        set_ui_props_async!(handle, platform, SystemPageData, platform_profile);
        set_ui_props_async!(
            handle,
            platform,
            SystemPageData,
            platform_profile_linked_epp
        );
        set_ui_props_async!(handle, platform, SystemPageData, profile_balanced_epp);
        set_ui_props_async!(handle, platform, SystemPageData, profile_performance_epp);
        set_ui_props_async!(handle, platform, SystemPageData, profile_quiet_epp);
        set_ui_props_async!(
            handle,
            platform,
            SystemPageData,
            platform_profile_on_battery
        );
        set_ui_props_async!(
            handle,
            platform,
            SystemPageData,
            change_platform_profile_on_battery
        );
        set_ui_props_async!(handle, platform, SystemPageData, platform_profile_on_ac);
        set_ui_props_async!(
            handle,
            platform,
            SystemPageData,
            change_platform_profile_on_ac
        );

        set_ui_props_async!(handle, platform, SystemPageData, enable_ppt_group);

        let platform_copy = platform.clone();
        handle
            .upgrade_in_event_loop(move |handle| {
                set_ui_callbacks!(handle,
                    SystemPageData(as bool),
                    platform_copy.enable_ppt_group(as bool),
                    "Applied PPT group settings {}",
                    "Setting PPT group settings failed"
                );

                set_ui_callbacks!(handle,
                    SystemPageData(as f32),
                    platform_copy.charge_control_end_threshold(as u8),
                    "Charge limit successfully set to {}",
                    "Setting Charge limit failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.platform_profile(.into()),
                    "Throttle policy set to {}",
                    "Setting Throttle policy failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.profile_balanced_epp(.into()),
                    "Throttle policy EPP set to {}",
                    "Setting Throttle policy EPP failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.profile_performance_epp(.into()),
                    "Throttle policy EPP set to {}",
                    "Setting Throttle policy EPP failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.profile_quiet_epp(.into()),
                    "Throttle policy EPP set to {}",
                    "Setting Throttle policy EPP failed"
                );
                set_ui_callbacks!(
                    handle,
                    SystemPageData(),
                    platform_copy.platform_profile_linked_epp(),
                    "Throttle policy linked to EPP: {}",
                    "Setting Throttle policy linked to EPP failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.platform_profile_on_ac(.into()),
                    "Throttle policy on AC set to {}",
                    "Setting Throttle policy on AC failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as bool),
                    platform_copy.change_platform_profile_on_ac(.into()),
                    "Throttle policy on AC enabled: {}",
                    "Setting Throttle policy on AC failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as i32),
                    platform_copy.platform_profile_on_battery(.into()),
                    "Throttle policy on abttery set to {}",
                    "Setting Throttle policy on battery failed"
                );
                set_ui_callbacks!(handle,
                    SystemPageData(as bool),
                    platform_copy.change_platform_profile_on_battery(.into()),
                    "Throttle policy on battery enabled: {}",
                    "Setting Throttle policy on AC failed"
                );
            })
            .ok();

        let armoury_attrs;
        if let Ok(attrs) = find_iface_async::<AsusArmouryProxy>("xyz.ljones.AsusArmoury").await {
            armoury_attrs = attrs;
            handle
                .upgrade_in_event_loop(|ui| {
                    ui.global::<SystemPageData>().set_asus_armoury_loaded(true)
                })
                .ok();
        } else {
            error!(
                "The kernel module asus-armoury is required, if you do not have this you will \
                 need to either build or install a kernel which includes the patchwork. This \
                 driver is in process of being upstreamed"
            );
            return;
        }

        for attr in armoury_attrs {
            if let Ok(value) = attr.current_value().await {
                let name = attr.name().await.unwrap();
                let platform = platform.clone();
                handle
                    .upgrade_in_event_loop(move |handle| match name {
                        FirmwareAttribute::ApuMem => {}
                        FirmwareAttribute::CoresPerformance => {}
                        FirmwareAttribute::CoresEfficiency => {}
                        FirmwareAttribute::PptPl1Spl => {
                            init_minmax_property!(ppt_pl1_spl, handle, attr);
                            setup_callback!(ppt_pl1_spl, handle, attr, i32);
                            setup_callback_restore_default!(ppt_pl1_spl, handle, attr);
                            setup_minmax_external!(ppt_pl1_spl, handle, attr, platform);
                        }
                        FirmwareAttribute::PptPl2Sppt => {
                            init_minmax_property!(ppt_pl2_sppt, handle, attr);
                            setup_callback!(ppt_pl2_sppt, handle, attr, i32);
                            setup_callback_restore_default!(ppt_pl2_sppt, handle, attr);
                            setup_minmax_external!(ppt_pl2_sppt, handle, attr, platform);
                        }
                        FirmwareAttribute::PptPl3Fppt => {
                            init_minmax_property!(ppt_pl3_fppt, handle, attr);
                            setup_callback!(ppt_pl3_fppt, handle, attr, i32);
                            setup_callback_restore_default!(ppt_pl3_fppt, handle, attr);
                            setup_minmax_external!(ppt_pl3_fppt, handle, attr, platform);
                        }
                        FirmwareAttribute::PptFppt => {
                            init_minmax_property!(ppt_fppt, handle, attr);
                            setup_callback!(ppt_fppt, handle, attr, i32);
                            setup_callback_restore_default!(ppt_fppt, handle, attr);
                            setup_minmax_external!(ppt_fppt, handle, attr, platform);
                        }
                        FirmwareAttribute::PptApuSppt => {
                            init_minmax_property!(ppt_apu_sppt, handle, attr);
                            setup_callback!(ppt_apu_sppt, handle, attr, i32);
                            setup_callback_restore_default!(ppt_apu_sppt, handle, attr);
                            setup_minmax_external!(ppt_apu_sppt, handle, attr, platform);
                        }
                        FirmwareAttribute::PptPlatformSppt => {
                            init_minmax_property!(ppt_platform_sppt, handle, attr);
                            setup_callback!(ppt_platform_sppt, handle, attr, i32);
                            setup_callback_restore_default!(ppt_platform_sppt, handle, attr);
                            setup_minmax_external!(ppt_platform_sppt, handle, attr, platform);
                        }
                        FirmwareAttribute::NvDynamicBoost => {
                            init_minmax_property!(nv_dynamic_boost, handle, attr);
                            setup_callback!(nv_dynamic_boost, handle, attr, i32);
                            setup_callback_restore_default!(nv_dynamic_boost, handle, attr);
                            setup_minmax_external!(nv_dynamic_boost, handle, attr, platform);
                        }
                        FirmwareAttribute::NvTempTarget => {
                            init_minmax_property!(nv_temp_target, handle, attr);
                            setup_callback!(nv_temp_target, handle, attr, i32);
                            setup_callback_restore_default!(nv_temp_target, handle, attr);
                            setup_minmax_external!(nv_temp_target, handle, attr, platform);
                        }
                        FirmwareAttribute::DgpuBaseTgp => {}
                        FirmwareAttribute::DgpuTgp => {}
                        FirmwareAttribute::ChargeMode => {}
                        FirmwareAttribute::BootSound => {
                            init_property!(boot_sound, handle, value, i32);
                            setup_callback!(boot_sound, handle, attr, i32);
                            setup_external!(boot_sound, i32, handle, attr, value)
                        }
                        FirmwareAttribute::McuPowersave => {}
                        FirmwareAttribute::PanelOverdrive => {
                            init_property!(panel_overdrive, handle, value, i32);
                            setup_callback!(panel_overdrive, handle, attr, i32);
                            setup_external!(panel_overdrive, i32, handle, attr, value)
                        }
                        FirmwareAttribute::PanelHdMode => {}
                        FirmwareAttribute::EgpuConnected => {}
                        FirmwareAttribute::EgpuEnable => {}
                        FirmwareAttribute::DgpuDisable => {}
                        FirmwareAttribute::GpuMuxMode => {}
                        FirmwareAttribute::MiniLedMode => {
                            init_property!(mini_led_mode, handle, value, i32);
                            setup_callback!(mini_led_mode, handle, attr, i32);
                            setup_external!(mini_led_mode, i32, handle, attr, value);
                        }
                        FirmwareAttribute::PendingReboot => {}
                        FirmwareAttribute::None => {}
                    })
                    .ok();
            }
        }
    });
}
