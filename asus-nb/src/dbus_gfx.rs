// This code was autogenerated with `dbus-codegen-rust -s -d org.asuslinux.Daemon -p /org/asuslinux/Gfx -m None -f org.asuslinux.Daemon -c blocking`, see https://github.com/diwic/dbus-rs
use dbus;
#[allow(unused_imports)]
use dbus::arg;
use dbus::blocking;

pub trait OrgAsuslinuxDaemon {
    fn set_vendor(&self, vendor: &str) -> Result<(), dbus::Error>;
}

impl<'a, T: blocking::BlockingSender, C: ::std::ops::Deref<Target = T>> OrgAsuslinuxDaemon
    for blocking::Proxy<'a, C>
{
    fn set_vendor(&self, vendor: &str) -> Result<(), dbus::Error> {
        self.method_call("org.asuslinux.Daemon", "SetVendor", (vendor,))
    }
}

#[derive(Debug)]
pub struct OrgAsuslinuxDaemonNotifyGfx {
    pub vendor: String,
}

impl arg::AppendAll for OrgAsuslinuxDaemonNotifyGfx {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.vendor, i);
    }
}

impl arg::ReadAll for OrgAsuslinuxDaemonNotifyGfx {
    fn read(i: &mut arg::Iter) -> Result<Self, arg::TypeMismatchError> {
        Ok(OrgAsuslinuxDaemonNotifyGfx { vendor: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgAsuslinuxDaemonNotifyGfx {
    const NAME: &'static str = "NotifyGfx";
    const INTERFACE: &'static str = "org.asuslinux.Daemon";
}
