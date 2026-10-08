//! The GMCP packages Vosh asks the game for.

#[test]
fn core_supports_set_names_every_package_vosh_reads() {
    let body = vosh_protocol::gmcp::build(
        "Core.Supports.Set",
        &super::REQUESTED_GMCP_PACKAGES.to_vec(),
    )
    .expect("the list serializes");
    let msg = vosh_protocol::gmcp::parse(&body).expect("the body parses");
    let modules: Vec<&str> = msg
        .data
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|v| v.as_str()?.split(' ').next())
        .collect();
    for read in [
        "Char.Vitals",
        "Char.Affects",
        "Char.Combat",
        "Char.Prompt",
        "Char.State",
        "Char.Worth",
        "Room.Info",
        "Room.Weather",
        "Comm.Channel",
        "World.Time",
        "World.Moons",
        "Map.Tiles",
        "Imm.Queues",
        "Group.Info",
        "Snoop.Output",
    ] {
        assert!(
            modules
                .iter()
                .any(|m| read == *m || read.starts_with(&format!("{m}."))),
            "Core.Supports.Set leaves out {read}"
        );
    }
}
