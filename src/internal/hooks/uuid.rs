use uuid::Uuid;

pub fn uuid_hook() {
    let uuid = Uuid::new_v4();
    println!("{}", uuid.to_string());
}
