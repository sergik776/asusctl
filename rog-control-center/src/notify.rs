use notify_rust::{Hint, Notification, NotificationHandle};
use rog_dbus::{
    zbus_anime::AnimeProxy, zbus_led::LedProxy, zbus_platform::RogBiosProxy,
    zbus_power::PowerProxy, zbus_profile::ProfileProxy,
};
use rog_profiles::Profile;
use smol::{future, Executor};
use std::{
    error::Error,
    fmt::Display,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::spawn,
};
use zbus::export::futures_util::StreamExt;

const NOTIF_HEADER: &str = "ROG Control";

macro_rules! notify {
    ($notifier:expr, $last_notif:ident) => {
        if let Some(notif) = $last_notif.take() {
            notif.close();
        }
        if let Ok(x) = $notifier {
            $last_notif.replace(x);
        }
    };
}

type SharedHandle = Arc<Mutex<Option<NotificationHandle>>>;

pub fn start_notifications(
    charge_notified: Arc<AtomicBool>,
    bios_notified: Arc<AtomicBool>,
    aura_notified: Arc<AtomicBool>,
    anime_notified: Arc<AtomicBool>,
    profiles_notified: Arc<AtomicBool>,
    _fans_notified: Arc<AtomicBool>,
    notifs_enabled: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let last_notification: SharedHandle = Arc::new(Mutex::new(None));

    let executor = Executor::new();
    // BIOS notif
    let last_notif = last_notification.clone();
    let notifs_enabled1 = notifs_enabled.clone();
    let bios_notified1 = bios_notified.clone();
    // TODO: make a macro or generic function or something...
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = RogBiosProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_post_boot_sound().await {
                p.for_each(|e| {
                    if let Ok(out) = e.args() {
                        if notifs_enabled1.load(Ordering::SeqCst) {
                            if let Ok(ref mut lock) = last_notif.try_lock() {
                                notify!(do_notification("BIOS Post sound", &out.on()), lock);
                            }
                        }
                        bios_notified1.store(true, Ordering::SeqCst);
                    }
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    let bios_notified1 = bios_notified.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = RogBiosProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_panel_od().await {
                p.for_each(|_| {
                    bios_notified1.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    let bios_notified1 = bios_notified.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = RogBiosProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_dgpu_disable().await {
                p.for_each(|_| {
                    bios_notified1.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    let bios_notified1 = bios_notified.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = RogBiosProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_egpu_enable().await {
                p.for_each(|_| {
                    bios_notified1.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    let bios_notified1 = bios_notified.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = RogBiosProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_gpu_mux_mode().await {
                p.for_each(|_| {
                    bios_notified1.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    // Charge notif
    let last_notif = last_notification.clone();
    let notifs_enabled1 = notifs_enabled.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = PowerProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_charge_control_end_threshold().await {
                p.for_each(|e| {
                    if let Ok(out) = e.args() {
                        if notifs_enabled1.load(Ordering::SeqCst) {
                            if let Ok(ref mut lock) = last_notif.try_lock() {
                                notify!(
                                    do_notification("Battery charge limit changed to", &out.limit),
                                    lock
                                );
                            }
                        }
                        charge_notified.store(true, Ordering::SeqCst);
                    }
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    // Profile notif
    let last_notif = last_notification.clone();
    let notifs_enabled1 = notifs_enabled.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = ProfileProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_profile().await {
                p.for_each(|e| {
                    if let Ok(out) = e.args() {
                        if notifs_enabled1.load(Ordering::SeqCst) {
                            if let Ok(ref mut lock) = last_notif.try_lock() {
                                notify!(do_thermal_notif(&out.profile), lock);
                            }
                        }
                        profiles_notified.store(true, Ordering::SeqCst);
                    }
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    // LED notif
    let last_notif = last_notification.clone();
    let aura_notif = aura_notified.clone();
    let notifs_enabled1 = notifs_enabled.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = LedProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_led().await {
                p.for_each(|e| {
                    if let Ok(out) = e.args() {
                        if notifs_enabled1.load(Ordering::SeqCst) {
                            if let Ok(ref mut lock) = last_notif.try_lock() {
                                notify!(
                                    do_notification(
                                        "Keyboard LED mode changed to",
                                        &out.data.mode_name()
                                    ),
                                    lock
                                );
                            }
                        }
                        aura_notif.store(true, Ordering::SeqCst);
                    }
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    let aura_notif = aura_notified.clone();
    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = LedProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_notify_led().await {
                p.for_each(|_| {
                    aura_notif.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = LedProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_all_signals().await {
                p.for_each(|_| {
                    aura_notified.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    executor
        .spawn(async move {
            let conn = zbus::Connection::system().await.unwrap();
            let proxy = AnimeProxy::new(&conn).await.unwrap();
            if let Ok(p) = proxy.receive_power_states().await {
                p.for_each(|_| {
                    anime_notified.store(true, Ordering::SeqCst);
                    future::ready(())
                })
                .await;
            };
        })
        .detach();

    spawn(move || loop {
        smol::block_on(executor.tick());
    });
    Ok(())
}

fn do_notification<T>(
    message: &str,
    data: &T,
) -> Result<NotificationHandle, notify_rust::error::Error>
where
    T: Display,
{
    Notification::new()
        .summary(NOTIF_HEADER)
        .body(&format!("{message} {data}"))
        .timeout(2000)
        .show()
}

fn do_thermal_notif(profile: &Profile) -> Result<NotificationHandle, Box<dyn Error>> {
    let icon = match profile {
        Profile::Balanced => "asus_notif_yellow",
        Profile::Performance => "asus_notif_red",
        Profile::Quiet => "asus_notif_green",
    };
    let profile: &str = (*profile).into();
    let x = Notification::new()
        .summary("ASUS ROG")
        .body(&format!(
            "Thermal profile changed to {}",
            profile.to_uppercase(),
        ))
        .hint(Hint::Resident(true))
        .timeout(2000)
        .hint(Hint::Category("device".into()))
        //.hint(Hint::Transient(true))
        .icon(icon)
        .show()?;
    Ok(x)
}
