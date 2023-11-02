use anyhow::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{result::Result::Ok, time::Duration, sync::Arc, collections::HashMap, net::{IpAddr, Ipv4Addr}, str::FromStr};
use cpal::*;

use tokio::{net::{UdpSocket, TcpStream, TcpListener}, sync::Mutex, task::JoinSet, io::{AsyncWriteExt, AsyncReadExt}, time::{error::Elapsed, sleep}, join};
use tokio::io::{AsyncBufReadExt, BufReader};


use rand::Rng;

use winping::{AsyncPinger, Buffer};

pub mod lightbulb;
use crate::lightbulb::*;

#[derive(Default)]
struct SoundCapture{
    stream: Option<Arc<std::sync::Mutex<Stream>>>,
    current_device: Option<Arc<Device>>,
    counter: Arc<std::sync::Mutex<u32>>
}

impl SoundCapture {
    pub fn new() -> Result<SoundCapture, Error> {
        let mut s_cap = SoundCapture {
            ..Default::default()
        };
        Ok(s_cap)
    }

    pub fn init(&mut self) -> Result<(), Error> {
        let available_hosts = cpal::available_hosts(); // Get all hosts

        let host = cpal::host_from_id(available_hosts[0])?; // Use first host

        let default_out = Arc::new(host.default_output_device().unwrap()); // Getting default output

        self.current_device = Some(default_out.clone());

        let config = default_out.default_output_config().unwrap().config();

        println!("{:?}", default_out.name().unwrap()); // Printing Selected Device
        println!("Loaded config");
        println!("buffer_size: {:?}", &config.buffer_size);
        println!("sample_rate: {:?}", &config.sample_rate);
        println!("channels: {:?}", &config.channels);
        let counter = self.counter.clone();
        let mutex_stream = Arc::new(std::sync::Mutex::new(default_out.build_input_stream(
            &config,
            move |data: &[f32], _: &InputCallbackInfo| {
                let mut counter = counter.lock().unwrap();
                *counter += data.len() as u32;
                println!("{}", counter);
            },
            move |err: StreamError| SoundCapture::error_callback(err),
            Some(Duration::from_millis(2000)),
        )?));
        
        self.stream = Some(mutex_stream.clone());

        let stream = mutex_stream.lock().unwrap();
        stream.play()?;
        Ok(())
    }

    // pub fn data_callback(&self, data: &[f32]) {
    //     println!("{}", data.iter().sum::<f32>() as f32 / data.len() as f32);
    // }

    pub fn error_callback(err: StreamError) {
        eprintln!("an error occurred on stream: {}", err);
    }
}






#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 102 times per second =>   Discretization rate = (101*960)/60 = 1616 Hz
    
    let mut s_cap = SoundCapture::new().unwrap();
    s_cap.init();
    sleep(Duration::from_secs(1)).await;
    
    let lbp = LightBulbProvider::new().await;
    let lbp_c1 = lbp.clone();
    let routine_h = tokio::spawn(async move {
        loop {
            let lbpc1 = lbp_c1.clone();
            let lbpc = lbpc1.clone();
            lbpc.discover_routine().await;
        }
    });

    // let another_routine = tokio::spawn(async move {
    //     let mut color_f = false;
    //     let mut hue: u8 = 1;
    //     let mut bright: u8 = 0;
    //     let mut b_flag = true;

    //     let mut wheel: u8 = 0;
    //     loop {
    //         tokio::time::sleep(Duration::from_millis(150)).await;
    //         // let r: u8 = rand::thread_rng().gen_range(0..255);
    //         // let g: u8 = rand::thread_rng().gen_range(0..255);
    //         // let b: u8 = rand::thread_rng().gen_range(0..255);
    //         // let color = RGBColor::new(r, g, b);

    //         // let color = RGBColor::new(r, g, b);
    //         // let color_w = RGBColor::new(0,255,0);
    //         // let color_b = RGBColor::new(0,0,255);
    //         // let color = if color_f {color_w} else {color_b};
    //         // color_f = !color_f;
    //         // println!("Generated color: #{:X} , ({},{},{})", &color.get24Bit(), &color.r, &color.g, &color.b );

    //         // if hue < 360{
    //         //     hue +=3;
    //         // }
    //         // else {
    //         //     hue=1;
    //         // }

    //         // if bright + 1 == 100 {
    //         //     b_flag = false;
    //         // }

    //         // if bright == 1{
    //         //     b_flag = true;
    //         // }

    //         // if b_flag {
    //         //     bright += 1;
    //         // }
    //         // else{
    //         //     bright -= 1;
    //         // }


    //         if wheel < 245 {
    //             wheel +=10;
    //         }
    //         else {
    //             wheel = 0;
    //         }

    //         let mut color = RGBColor::new(0, 0, 0);
    //         color.wheel24bit(wheel);

    //         // println!("bright: {}", hue);
    //         let lbpc = lbp.clone();

    //         lbpc.clone().set_color_for_all(color.clone(), Duration::from_millis(500)).await;
    //         // lbpc.clone().set_hsv_color_for_all(hue, 99).await;
    //         // lbpc.clone().set_brightness_for_all(bright).await;
    //     }
    // });

    // let _ = tokio::join!(routine_h);//, another_routine);



    Ok(())
}