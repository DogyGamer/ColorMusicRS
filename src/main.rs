use anyhow::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{result::Result::Ok, time::{Duration, self}, sync::Arc, collections::HashMap, net::{IpAddr, Ipv4Addr}, str::FromStr, future};
use cpal::*;

use tokio::{net::{UdpSocket, TcpStream, TcpListener}, sync::Mutex, task::{JoinSet, futures}, io::{AsyncWriteExt, AsyncReadExt}, time::{error::Elapsed, sleep}, join};
use tokio::io::{AsyncBufReadExt, BufReader};


use rand::Rng;

use winping::{AsyncPinger, Buffer};

pub mod lightbulb;
pub mod vol_analyzer;

use crate::lightbulb::*;
use crate::vol_analyzer::*;


#[derive(Default)]
struct SoundCapture{
    stream: Option<Arc<Mutex<Stream>>>,
    current_device: Option<Arc<Device>>,
    vol_analyzer: Option<Arc<std::sync::Mutex<VolAnalyzer>>>,
    lightbulb_provider: Option<Arc<LightBulbProvider>>
}

impl SoundCapture {
    pub fn new(lbp: Arc<LightBulbProvider>) -> Result<SoundCapture, Error> {
        let mut vol_anal = VolAnalyzer::new();
        vol_anal.set_vol_k(1);
        vol_anal._trsh = 1;
        vol_anal._volMin = 10;
        vol_anal._volMax = 100;
        vol_anal._pulseTrsh = 90;
        vol_anal._pulseMin = 80;
        vol_anal._pulseTout = Duration::from_millis(10);


        let s_cap = SoundCapture {
            vol_analyzer: Some(Arc::new(std::sync::Mutex::new(vol_anal))),
            lightbulb_provider: Some(lbp.clone()),
            ..Default::default()
        };
        Ok(s_cap)
    }

    pub async fn init(&mut self) -> Result<(), Error> {
        let available_hosts = cpal::available_hosts(); // Get all hosts

        let host = cpal::host_from_id(available_hosts[0])?; // Use first host

        let default_out = Arc::new(host.default_output_device().unwrap()); // Getting default output

        self.current_device = Some(default_out.clone());

        let config = default_out.default_output_config().unwrap().config();


        // let a = time::SystemTime::now();
        // a.duration_since(earlier)

        println!("{:?}", default_out.name().unwrap()); // Printing Selected Device
        println!("Loaded config");
        println!("buffer_size: {:?}", &config.buffer_size);
        println!("sample_rate: {:?}", &config.sample_rate);
        println!("channels: {:?}", &config.channels);
        
        let analyzer = self.vol_analyzer.clone().unwrap();
        let mut counter: u8 = 0;
        let lbp = self.lightbulb_provider.clone();
        let mutex_stream = Arc::new(Mutex::new(default_out.build_input_stream(
            &config,
            move |data: &[f32], _:&_| {
                let mut analyzer = analyzer.lock().unwrap();
                let lbp = lbp.clone();
                // println!("{}", data.len());
                for val in data {
                    let i_val: i32 = (val * 100000000.0) as i32;
                    let res = analyzer.tick(i_val.clone());
                    if res {
                        println!("{}", analyzer.get_vol());
                        let mut inc = 2;
                        let mut color = RGBColor::new(255, 0,0);
                        if analyzer.get_pulse() {
                            inc = 129;
                            println!("Pulse");
                            color = RGBColor::new(70, 40, 255);
                        }

                        if counter > 255 - inc {
                            counter = (counter as u16 + inc as u16 - 255) as u8;
                        } else {
                            counter = (counter as u16 + inc as u16) as u8;
                        }

                        
                        color.wheel24bit(counter);

                        tokio::runtime::Builder::new_current_thread()
                        .enable_time()
                        .build()
                        .unwrap()
                        .block_on(
                            lbp.clone().unwrap().set_color_for_all(color, Duration::from_millis(350))
                        );
                        tokio::runtime::Builder::new_current_thread()
                        .enable_time()
                        .build()
                        .unwrap()
                        .block_on(
                            lbp.clone().unwrap().set_brightness_for_all(analyzer.get_vol() as u8, Duration::from_millis(200))
                        );
                    }
                }
            },
            move |err: StreamError| SoundCapture::error_callback(err),
            Some(Duration::from_millis(2000)),
        )?));
        
        self.stream = Some(mutex_stream.clone());

        let stream = mutex_stream.lock().await;
        stream.play()?;
        Ok(())
    }

    pub fn error_callback(err: StreamError) {
        eprintln!("an error occurred on stream: {}", err);
    }

}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 102 times per second =>   Discretization rate = (101*960)/60 = 1616 Hz

    // sleep(Duration::from_secs(1)).await;
    let lbp = LightBulbProvider::new().await;
    let lbp_c1 = lbp.clone();
    let routine_h = tokio::spawn(async move {
        loop {
            let lbpc1 = lbp_c1.clone();
            let lbpc = lbpc1.clone();
            lbpc.discover_routine().await;
        }
    });
    
    let mut s_cap = SoundCapture::new(lbp).unwrap();
    let _ = s_cap.init().await;
    
    let _ = tokio::join!(routine_h);//, another_routine);
    Ok(())
}