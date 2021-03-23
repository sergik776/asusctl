use daemon::ctrl_leds::{CtrlKbdBacklight, DbusKbdBacklight};
use daemon::{
    config::Config, ctrl_supported::SupportedFunctions, laptops::print_board_info, GetSupported,
};
use daemon::{config_aura::AuraConfig, ctrl_charge::CtrlCharge};
use daemon::{ctrl_anime::CtrlAnimeDisplay, ctrl_gfx::gfx::CtrlGraphics};
use daemon::{
    ctrl_fan_cpu::{CtrlFanAndCPU, DbusFanAndCpu},
    laptops::LaptopLedData,
};

use daemon::{CtrlTask, Reloadable, ZbusAdd};
use log::LevelFilter;
use log::{error, info, warn};
use rog_dbus::DBUS_NAME;
use rog_types::gfx_vendors::GfxVendors;
use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use daemon::ctrl_rog_bios::CtrlRogBios;
use std::convert::Into;
use zbus::fdo;
use zbus::Connection;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut logger = env_logger::Builder::new();
    logger
        .target(env_logger::Target::Stdout)
        .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
        .filter(None, LevelFilter::Info)
        .init();

    info!("   daemon v{}", daemon::VERSION);
    info!(" rog-dbus v{}", rog_dbus::VERSION);
    info!("rog-types v{}", rog_types::VERSION);

    start_daemon()?;
    Ok(())
}

// Timing is such that:
// - interrupt write is minimum 1ms (sometimes lower)
// - read interrupt must timeout, minimum of 1ms
// - for a single usb packet, 2ms total.
// - to maintain constant times of 1ms, per-key colours should use
//   the effect endpoint so that the complete colour block is written
//   as fast as 1ms per row of the matrix inside it. (10ms total time)
fn start_daemon() -> Result<(), Box<dyn Error>> {
    let supported = SupportedFunctions::get_supported();
    print_board_info();
    println!("{}", serde_json::to_string_pretty(&supported).unwrap());

    let config = Config::load();
    let enable_gfx_switching = config.gfx_managed;
    let config = Arc::new(Mutex::new(config));

    let connection = Connection::new_system()?;
    fdo::DBusProxy::new(&connection)?
        .request_name(DBUS_NAME, fdo::RequestNameFlags::ReplaceExisting.into())?;
    let mut object_server = zbus::ObjectServer::new(&connection);

    supported.add_to_server(&mut object_server);

    match CtrlRogBios::new(config.clone()) {
        Ok(mut ctrl) => {
            // Do a reload of any settings
            ctrl.reload()
                .unwrap_or_else(|err| warn!("Battery charge limit: {}", err));
            // Then register to dbus server
            ctrl.add_to_server(&mut object_server);
        }
        Err(err) => {
            error!("rog_bios_control: {}", err);
        }
    }

    match CtrlCharge::new(config.clone()) {
        Ok(mut ctrl) => {
            // Do a reload of any settings
            ctrl.reload()
                .unwrap_or_else(|err| warn!("Battery charge limit: {}", err));
            // Then register to dbus server
            ctrl.add_to_server(&mut object_server);
        }
        Err(err) => {
            error!("charge_control: {}", err);
        }
    }

    match CtrlAnimeDisplay::new() {
        Ok(ctrl) => {
            ctrl.add_to_server(&mut object_server);
        }
        Err(err) => {
            error!("AniMe control: {}", err);
        }
    }

    if enable_gfx_switching {
        match CtrlGraphics::new(config.clone()) {
            Ok(mut ctrl) => {
                // Need to check if a laptop has the dedicated gfx switch
                if CtrlRogBios::has_dedicated_gfx_toggle() {
                    if let Ok(ded) = CtrlRogBios::get_gfx_mode() {
                        if let Ok(vendor) = ctrl.get_gfx_mode() {
                            if ded == 1 && vendor != GfxVendors::Nvidia {
                                warn!("Dedicated GFX toggle is on but driver mode is not nvidia \nSetting to nvidia driver mode");
                                let devices = ctrl.devices();
                                let bus = ctrl.bus();
                                CtrlGraphics::do_vendor_tasks(GfxVendors::Nvidia, &devices, &bus)?;
                            } else if ded == 0 {
                                info!("Dedicated GFX toggle is off");
                            }
                        }
                    }
                }
                ctrl.reload()
                    .unwrap_or_else(|err| error!("Gfx controller: {}", err));
                ctrl.add_to_server(&mut object_server);
            }
            Err(err) => {
                error!("Gfx control: {}", err);
            }
        }
    }

    // Collect tasks for task thread
    let mut tasks: Vec<Arc<Mutex<dyn CtrlTask + Send>>> = Vec::new();

    if let Ok(mut ctrl) = CtrlFanAndCPU::new(config).map_err(|err| {
        error!("Profile control: {}", err);
    }) {
        ctrl.reload()
            .unwrap_or_else(|err| warn!("Profile control: {}", err));
        let tmp = Arc::new(Mutex::new(ctrl));
        DbusFanAndCpu::new(tmp).add_to_server(&mut object_server);
    };

    let laptop = LaptopLedData::get_data();
    let aura_config = AuraConfig::load(&laptop);
    if let Ok(ctrl) = CtrlKbdBacklight::new(laptop, aura_config).map_err(|err| {
        error!("Keyboard control: {}", err);
        err
    }) {
        let tmp = Arc::new(Mutex::new(ctrl));
        DbusKbdBacklight::new(tmp.clone()).add_to_server(&mut object_server);
        tasks.push(tmp);
    }

    // TODO: implement messaging between threads to check fails
    // These tasks generally read a sys path or file to check for a
    // change
    let _handle = std::thread::Builder::new()
        .name("asusd watch".to_string())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));

            for ctrl in tasks.iter() {
                if let Ok(mut lock) = ctrl.try_lock() {
                    lock.do_task()
                        .map_err(|err| {
                            warn!("do_task error: {}", err);
                        })
                        .ok();
                }
            }
        });

    object_server
        .with("/org/asuslinux/Charge", |obj: &CtrlCharge| {
            let x = obj.limit();
            obj.notify_charge(x as u8)
        })
        .map_err(|err| {
            warn!("object_server notify_charge error: {}", err);
        })
        .ok();

    loop {
        if let Err(err) = object_server.try_handle_next() {
            eprintln!("{}", err);
        }
    }
}
