use sysinfo::Components;

fn main() {
    let components = Components::new_with_algo(true);
    println!("Components count: {}", components.list().len());
    for component in components.list() {
        println!("{:?} {}°C", component.label(), component.temperature());
    }
}
