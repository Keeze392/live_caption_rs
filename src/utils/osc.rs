use rosc::{OscMessage, OscPacket, OscType, encoder};
use serde::{Serialize, Deserialize};
use std::{net::UdpSocket};

#[derive(Default, Serialize, Deserialize)]
pub struct OSCAddress {
    pub path: String,
    pub port: String,
}

pub struct OSCSender {
    osc_address: OSCAddress,
    socket: UdpSocket,
}

impl OSCSender {
    pub fn new() -> Self {
        Self {
            osc_address: OSCAddress::default(),

            socket: UdpSocket::bind("127.0.0.1:0").expect("OSC Err -- socket bind local ip failed"),
        }
    }

    pub fn set_path(&mut self, output_path: String) {
        self.osc_address.path = output_path;
    }

    pub fn set_port(&mut self, output_port: String) {
        // not accept any char other than numbers
        if !output_port.chars().all(|val| val.is_numeric()) {
            eprintln!("ERR -- OSC port accept only numbers");
            return;
        }

        self.osc_address.port = output_port;
    }

    // for sending with path and port to local ip only
    pub fn send(&self, text: String) {
        let msg = OscMessage {
            addr: self.osc_address.path.clone(),
            args: vec![OscType::String(text)],
        };

        let packet = OscPacket::Message(msg);

        let buf = match encoder::encode(&packet) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("OSC Err -- encoding a packet failed: {e}");
                return;
            }
        };

        if !self.osc_address.port.is_empty() {
            let full_addr_target = format!("127.0.0.1:{}", self.osc_address.port);

            match self.socket.send_to(&buf, full_addr_target) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("OSC Err -- Sending failed: {e}");
                    return;
                }
            }
        }
    }

    // plan to add for vrc in future
    pub fn _send_to_vrc(&self) {
        unimplemented!()
    }
}

impl Default for OSCSender {
    fn default() -> Self {
        Self {
            osc_address: OSCAddress::default(),
            socket: UdpSocket::bind("127.0.0.1:0").expect("OSC Err -- socket bind local ip failed"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn set_path_test() {
        let mut osc_struct_test = OSCSender::new();

        assert_eq!(String::from(""), osc_struct_test.osc_address.path);

        osc_struct_test.set_path("/blah_blah".into());

        assert_eq!(String::from("/blah_blah"), osc_struct_test.osc_address.path);
    }

    #[test]
    fn set_port_test() {
        let mut osc_struct_test = OSCSender::new();

        assert_eq!(String::from(""), osc_struct_test.osc_address.port);

        osc_struct_test.set_port("9009".into());

        assert_eq!(String::from("9009"), osc_struct_test.osc_address.port);

        osc_struct_test.set_port("abc".into());

        assert_ne!("abc", osc_struct_test.osc_address.port);
    }

    // i don't know if this is good idea or design
    // just checking is ipv4 if true meaning it's available i guess?
    #[test]
    fn socket_test() {
        let osc_struct_test = OSCSender::new();

        assert!(osc_struct_test.socket.local_addr().unwrap().is_ipv4());
    }

    #[tokio::test]
    async fn send_test() {
        use rosc::decoder;
        use tokio::net;

        let receiver = net::UdpSocket::bind("127.0.0.1:9005")
            .await
            .expect("test -- failing bind udp socket in test");

        let mut buf: [u8; 32] = [0; 32];

        let mut test_sender = OSCSender::new();

        test_sender.set_path("/say_hi".into());
        test_sender.set_port("9005".into());

        test_sender.send("hello!".into());

        receiver
            .recv(&mut buf)
            .await
            .expect("test -- recv failed");

        let msg = decoder::decode_udp(&buf)
            .expect("test -- decoding failed")
            .1;

        assert_eq!(String::from("/say_hi, (s) hello!"), msg.to_string());
    }

    #[tokio::test]
    #[ignore]
    async fn send_to_vrc_test() {

    }
}
