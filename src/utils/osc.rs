use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{net::UdpSocket, sync::{Arc, Mutex}};

pub struct OSCSender {
    osc_output_path: String,
    osc_output_port: String,

    socket: UdpSocket,
}

impl OSCSender {
    pub fn new(arc_output_path: &Arc<Mutex<String>>, arc_output_port: &Arc<Mutex<String>>) -> Self {
        let output_path = arc_output_path.lock().unwrap().clone();
        let output_port = arc_output_port.lock().unwrap().clone();

        Self {
            osc_output_path: output_path,
            osc_output_port: output_port,

            socket: UdpSocket::bind("127.0.0.1:0").expect("OSC Err -- socket bind local ip failed"),
        }
    }

    pub fn set_path(&mut self, arc_output_path: &Arc<Mutex<String>>) {
        self.osc_output_path = arc_output_path.lock().unwrap().clone();
    }

    pub fn set_port(&mut self, arc_output_port: &Arc<Mutex<String>>) {
        let temp = arc_output_port.lock().unwrap().clone();

        // not accept any char other than numbers
        if !temp.chars().all(|val| val.is_numeric()) {
            eprintln!("ERR -- OSC port accept only numbers");
            return;
        }

        self.osc_output_port = temp;
    }

    // for sending with path and port to local ip only
    pub fn send(&self, text: String) {
        let msg = OscMessage {
            addr: self.osc_output_path.clone(),
            args: vec![OscType::String(text)]
        };

        let packet = OscPacket::Message(msg);

        let buf = match encoder::encode(&packet) {
            Ok(val) => val,
            Err(e) => { eprintln!("OSC Err -- encoding a packet failed: {e}"); return; }
        };

        if !self.osc_output_port.is_empty() {
            let full_addr_target = format!("127.0.0.1:{}", self.osc_output_port);

            match self.socket.send_to(&buf, full_addr_target) {
                Ok(_) => (),
                Err(e) => { eprintln!("OSC Err -- Sending failed: {e}"); return; }
            }
        }
    }

    // plan to add for vrc in future
    pub fn send_to_vrc(&self) {
        unimplemented!()
    }
}

impl Default for OSCSender {
    fn default() -> Self {
        Self {
            osc_output_path: String::new(),
            osc_output_port: String::new(),

            socket: UdpSocket::bind("127.0.0.1:0").expect("OSC Err -- socket bind local ip failed"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn set_path_test() {
        let mut osc_struct_test = OSCSender::new(
            &Arc::new(Mutex::new("".into())),
            &Arc::new(Mutex::new("".into()))
        );

        assert_eq!(String::from(""), osc_struct_test.osc_output_path);

        osc_struct_test.set_path(&Arc::new(Mutex::new("/blah_blah".into())));
        
        assert_eq!(String::from("/blah_blah"), osc_struct_test.osc_output_path);
    }

    #[test]
    #[should_panic]
    fn set_port_test_fail() {
        let mut osc_struct_test = OSCSender::new(
            &Arc::new(Mutex::new("".into())),
            &Arc::new(Mutex::new("".into()))
        );

        osc_struct_test.set_port(&Arc::new(Mutex::new("abc".into())));

        assert_eq!("abc", osc_struct_test.osc_output_port);
    }

    #[test]
    fn set_port_test() {
        let mut osc_struct_test = OSCSender::new(
            &Arc::new(Mutex::new("".into())),
            &Arc::new(Mutex::new("".into()))
        );

        assert_eq!(String::from(""), osc_struct_test.osc_output_port);

        osc_struct_test.set_port(&Arc::new(Mutex::new("9009".into())));
        
        assert_eq!(String::from("9009"), osc_struct_test.osc_output_port);
    }

    // i don't know if this is good idea or design
    // just checking is ipv4 if true meaning it's available i guess?
    #[test]
    fn socket_test() {
        let osc_struct_test = OSCSender::new(
            &Arc::new(Mutex::new("".into())),
            &Arc::new(Mutex::new("".into()))
        );

        assert!(osc_struct_test.socket.local_addr().unwrap().is_ipv4());
    }

    #[test]
    #[ignore]
    fn send_test() {
        let osc_struct_test = OSCSender::new(
            &Arc::new(Mutex::new("/say_hi".into())),
            &Arc::new(Mutex::new("9005".into()))
        );

        osc_struct_test.send("test test".into());
    }

    #[test]
    #[ignore]
    fn send_to_vrc_test() {

    }
}
