use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{net::UdpSocket, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc}, thread::sleep, time::Duration};

pub fn osc_sender_string(
    text: mpsc::Receiver<String>,
    is_should_close: Arc<AtomicBool>,
    osc_output_path: Arc<Mutex<String>>,
    osc_output_port: Arc<Mutex<String>>,
    ) {

    // init
    let target_addr: String = String::from("127.0.0.1:");

    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    sleep(Duration::from_millis(1000));

    while !is_should_close.load(Ordering::Relaxed) {
        let port = osc_output_port.lock().unwrap().clone();
        let target_addr_port = format!("{}{}", target_addr, port);

        let addr_path_sender = osc_output_path.lock().unwrap().clone();
    
        let output_text: String = match text.recv() {
            Ok(s) => s,
            Err(_) => break,
        };

        let msg = OscMessage {
            addr: addr_path_sender.clone(),
            args: vec![OscType::String(output_text.clone())],
        };

        let packet = OscPacket::Message(msg);
        let buf = match encoder::encode(&packet) {
            Ok(b) => b,
            Err(e) => { println!("Error -- make packet failed: {e}"); return; }
        };

        socket.send_to(&buf, target_addr_port).unwrap();
    }
}
