use sysinfo::{System, Networks, Components};

fn main() {
    let mut sys = System::new_all();
    let mut networks = Networks::new_with_resolved_macs();
    let mut components = Components::new_with_sysinfo();

    sys.refresh_all();
    networks.refresh_list();
    components.refresh_list();

    let cpu_usage = sys.global_cpu_usage();
    let uptime = sysinfo::System::uptime();
}
