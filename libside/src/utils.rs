use std::io::{BufRead, BufReader, Read};

pub struct EtcUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String,
    pub home: String,
    pub shell: String,
}

pub struct EtcGroup {
    pub name: String,
    pub gid: u32,
    pub users: Vec<String>,
}

pub(crate) fn parse_etc_passwd(reader: impl Read) -> Result<Vec<EtcUser>, std::io::Error> {
    BufReader::new(reader)
        .lines()
        .map(|v| {
            v.map(|line| {
                let mut parts = line.split(":");
                let name = parts.next().unwrap();
                parts.next().unwrap();
                let uid = parts.next().unwrap();
                let gid = parts.next().unwrap();
                let gecos = parts.next().unwrap();
                let home = parts.next().unwrap();
                let shell = parts.next().unwrap();

                EtcUser {
                    name: name.to_string(),
                    uid: uid.parse::<u32>().unwrap(),
                    gid: gid.parse::<u32>().unwrap(),
                    gecos: gecos.to_string(),
                    home: home.to_string(),
                    shell: shell.to_string(),
                }
            })
        })
        .collect()
}

pub(crate) fn parse_etc_group(reader: impl Read) -> Result<Vec<EtcGroup>, std::io::Error> {
    BufReader::new(reader)
        .lines()
        .map(|v| {
            v.map(|line| {
                let mut parts = line.split(":");
                let name = parts.next().unwrap();
                parts.next().unwrap();
                let gid = parts.next().unwrap();
                let users = parts
                    .next()
                    .unwrap()
                    .split(",")
                    .map(|v| v.to_string())
                    .collect();

                EtcGroup {
                    name: name.to_string(),
                    gid: gid.parse::<u32>().unwrap(),
                    users,
                }
            })
        })
        .collect()
}
