use clap::{Parser};
use pnet::datalink::{self, Channel::Ethernet, Config};
use pnet::packet::{ethernet::EthernetPacket, MutablePacket};
use pnet::packet::Packet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;
use tokio::runtime::Runtime;
use std::env;
use log::{info, error,debug};
use env_logger::Builder;
/// Command-line arguments for the program
#[derive(Parser)]
#[command(name = "Packet Forwarder")]
#[command(about = "Packet forwarder between two network interfaces.")]
struct Args {
    /// Name of the external network interface
    #[arg(long)]
    external_iface: String,

    /// Name of the internal network interface
    #[arg(long)]
    internal_iface: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env::set_var("RUST_BACKTRACE", "1");
     // Initialize env_logger
       // You can set the level in code here
    Builder::new()
    .filter_level(log::LevelFilter::Debug)  // Set to Debug level in code
    .init();
    // Parse command-line arguments using clap
    let args = Args::parse();

    // Select network interfaces by name
    let interfaces = datalink::interfaces();
      // Get the network interfaces inside the async block to ensure it lives long enough
      let interfaces = datalink::interfaces();
    
         // Find the external interface
    let external_iface = interfaces
    .iter()
    .find(|iface| iface.name == args.external_iface)
    .expect("No matching external interface found")
    .clone();  // Clone the interface to avoid borrowing issues

// Find the internal interface
let internal_iface = interfaces
    .iter()
    .find(|iface| iface.name == args.internal_iface)
    .expect("No matching internal interface found")
    .clone();  // Clone the interface to avoid borrowing issues
    info!("Using interfaces: {},ip:{:?} and {}, ip:{:?}", external_iface.name,external_iface.ips ,internal_iface.name,internal_iface.ips);

    // Create channels for both interfaces
    let config = Config::default();
    let (mut external_tx, mut external_rx) = match datalink::channel(&external_iface, config.clone()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!("Failed to create datalink channel for {}: {}", external_iface.name, e),
    };
    let (mut internal_tx, internal_rx) = match datalink::channel(&internal_iface, config) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!("Failed to create datalink channel for {}: {}", internal_iface.name, e),   
    };
 // Log some messages
    // // Wrap receivers in Arc<Mutex>
    // let external_rx = Arc::new(Mutex::new(external_rx));
    // let internal_rx = Arc::new(Mutex::new(internal_rx));
    
    // // Create mpsc channels for forwarding packets
    let (external_queue_tx, mut external_queue_rx) = mpsc::channel::<EthernetPacket>(100);
    let (internal_queue_tx, mut internal_queue_rx) = mpsc::channel::<EthernetPacket>(100);

    // // Spawn task for receiving packets on external_iface
    let external_iface_arc = Arc::new(external_iface.clone());
    let external_iface_arc_clone = Arc::clone(&external_iface_arc);

    task::spawn(async move {
        loop {
            match external_rx.next() {
                Ok(packet) => {
                    if let Some(ethernet_packet) = EthernetPacket::new(packet) {
                        debug!("Received packet on {}: {:?}", external_iface_arc.as_ref().name, ethernet_packet);
                        // Apply filtering logic here
                        if should_forward(&ethernet_packet) {
                            if let Err(e) = external_queue_tx.send(ethernet_packet.to_immutable()).await {
                                error!("Failed to send packet from {}: {}",external_iface_arc.as_ref().name, e);
                            }
                        } else {
                            debug!("Packet dropped by filter");
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read packet on {}: {}", external_iface.name, e);
                }
            }
        }
    });

    

    // // Spawn task for forwarding packets from external_iface to internal_iface
    task::spawn(async move {

        while let Some(packet) = external_queue_rx.recv().await {
            debug!("Forwarding packet from {} to {}: {:?}", external_iface_arc_clone.as_ref().name, internal_iface.name, packet);

            let mut buffer = vec![0u8; packet.packet().len()];
            buffer.copy_from_slice(packet.packet());
            if let Some(Err(e)) = internal_tx.send_to(&buffer, Some((*external_iface_arc_clone).clone())) {
 
                error!("Failed to send packet to {}: {}", internal_iface.name, e);
            }
        }
    });



    // // Keep the runtime running
    // Runtime::new()?.block_on(tokio::signal::ctrl_c())?;
    Ok(())
}

fn should_forward(packet: &EthernetPacket) -> bool {
    // Example filter: Forward only packets with a specific EtherType (e.g., IPv4)
    packet.get_ethertype().0 == 0x0800
}
