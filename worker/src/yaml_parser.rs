use std::io::Read;

use crate::types::config::Config;

fn get_file_content(path: &String) -> String {
    let mut f = std::fs::File::open(path).expect("error reading the yaml file");
    let mut content: String = String::new();
    f.read_to_string(&mut content)
        .expect("Unable to read yml data");
    content
}

pub fn get_conf(path: &String) -> Config {
    let content = get_file_content(path);
    let deserialized_point: Config = serde_yaml::from_str(&content).expect("error parsing yaml");
    // println!("{:?}", deserialized_point);
    deserialized_point
}
