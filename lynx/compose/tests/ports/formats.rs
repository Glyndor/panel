use lynx_compose::compose::types::PortMapping;
use lynx_compose::ports::parse_ports;

fn short(s: &str) -> PortMapping {
    PortMapping::Short(s.to_string())
}

#[test]
fn container_only() {
    let ports = parse_ports(&[short("80")]).unwrap();
    assert_eq!(ports[0].container_port, 80);
    assert_eq!(ports[0].protocol, "tcp");
    assert!(ports[0].host_port.is_none());
}

#[test]
fn host_colon_container() {
    let ports = parse_ports(&[short("8080:80")]).unwrap();
    assert_eq!(ports[0].container_port, 80);
    assert_eq!(ports[0].host_port, Some(8080));
}

#[test]
fn ip_host_container() {
    let ports = parse_ports(&[short("127.0.0.1:8080:80")]).unwrap();
    assert_eq!(ports[0].container_port, 80);
    assert_eq!(ports[0].host_port, Some(8080));
    assert_eq!(ports[0].host_ip, "127.0.0.1");
}

#[test]
fn udp_protocol() {
    assert_eq!(parse_ports(&[short("514:514/udp")]).unwrap()[0].protocol, "udp");
}

#[test]
fn container_only_udp() {
    let ports = parse_ports(&[short("53/udp")]).unwrap();
    assert_eq!(ports[0].container_port, 53);
    assert_eq!(ports[0].protocol, "udp");
    assert!(ports[0].host_port.is_none());
}

#[test]
fn range_expansion() {
    let ports = parse_ports(&[short("8000-8002:8000-8002")]).unwrap();
    assert_eq!(ports.len(), 3);
    assert_eq!(ports[0].container_port, 8000);
    assert_eq!(ports[1].container_port, 8001);
    assert_eq!(ports[2].container_port, 8002);
}
