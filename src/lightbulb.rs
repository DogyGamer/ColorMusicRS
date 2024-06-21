use anyhow::*;
use rand::Rng;
use winping::{AsyncPinger, Buffer};
use std::{result::Result::Ok, time::Duration, sync::Arc, collections::HashMap, net::{IpAddr, Ipv4Addr}, str::FromStr};
use tokio::{net::{UdpSocket, TcpStream, TcpListener}, sync::Mutex, task::JoinSet, io::{AsyncWriteExt, AsyncReadExt}, time::{error::Elapsed, sleep}};



#[derive(Clone, Copy, Default)]
pub struct RGBColor {
    r: u8,
    g: u8,
    b: u8,
}
impl RGBColor {
    pub fn new(r:u8, g:u8, b:u8) -> RGBColor{
        RGBColor {
            r: r,
            g: g,
            b: b,
        }
    }

    pub fn get24Bit(&self) -> u32{
        return (self.r.clone() as u32 * 65536u32) as u32 + (self.g.clone() as u32 *256u32) as u32 + self.b.clone() as u32;
    }


    pub fn wheel24bit(&mut self, color: u8){
        // 16777216
        let shift: u8;
        if (color > 170) {
            shift = (color - 170) * 3;
            self.r = shift;
            self.g = 0;
            self.b = 255 - shift;
        } else if (color > 85) {
            shift = (color - 85) * 3;
            self.r = 0;
            self.g = 255 - shift;
            self.b = shift;
        } else {
            shift = color * 3;
            self.r = 255 - shift;
            self.g = shift;
            self.b = 0;
        }
        // self.fade_8_local(br);
    }

    // fn fade_8(self, mut x:u8, br:u8){
    //     x = ((x as u16 * (br as u16 + 1)) >> 8) as u8
    // }

    // fn fade_8_local(&self, br: Option<u8>){
    //     if !br.is_none() {
    //         self.fade_8(self.r, br.unwrap());
    //         self.fade_8(self.g, br.unwrap());
    //         self.fade_8(self.b, br.unwrap());
    //     }
    // }

}

#[derive(Default, Clone,Copy, PartialEq, Debug)]
pub enum LightBulbModes {
    MusicMode,
    StandartMode,
    #[default]
    NoneMode
}

#[derive(Default, Clone)]
pub struct LightBulb{
    id: String,
    ip: String,
    music_mode_ip: String,
    music_mode_port: i32,
    socket: Option<Arc<Mutex<TcpStream>>>,
    current_state: LightBulbModes
}

impl LightBulb {
    async fn parse(response: String) -> Result<LightBulb, Error>{
        let mut parsed_bulb: LightBulb = LightBulb::default();

        let mut headers: HashMap<String, String> = HashMap::new();
        
        let lines: Vec<&str> = response.split('\n').collect();
        for line in lines[2..].iter(){
            let a = line.clone().clone();
            let ind = a.find(':');
            let _ = match ind {
                Some(n) => {
                    let k = a[..n].to_string();
                    let mut v = a[n+2..].to_string();
                    v.pop();
                    headers.insert(k, v)
                } 
                None => None,
            };
        }        
        let ip_startindex: usize = headers["Location"].find("//").unwrap()+2;
        

        parsed_bulb.ip = headers["Location"][ip_startindex..].to_string().clone();
        parsed_bulb.id = headers["id"].to_string();

        let port = 1488 + rand::thread_rng().gen_range(0..10);
        parsed_bulb.music_mode_port = port;
        parsed_bulb.music_mode_ip = "192.168.0.133".to_string();


        Ok(parsed_bulb)
    }
    pub async fn init_connection(&mut self) -> Result<(), Error>{
        self.switch_connection_mode(LightBulbModes::MusicMode).await
    }

    pub async fn connect_to_lb(&mut self)-> Result<(), Error>{
        let mut is_writable = false;
        if !self.socket.is_none() {
            is_writable =  self.socket.clone().unwrap().lock().await.writable().await.is_ok();
        }
        if self.current_state == LightBulbModes::StandartMode && is_writable{
            return Ok(());
        }
        let stream = TcpStream::connect(self.ip.clone().to_string().clone()).await?;
        self.socket = Some(Arc::new(Mutex::new(stream)));
        self.current_state = LightBulbModes::StandartMode.clone();
        Ok(())
    }

    pub async fn startMusicMode(&mut self) -> Result<(), Error> {
        if self.current_state == LightBulbModes::MusicMode {
            return Ok(());
        }
        println!("Trying to init MusicMode for {}", self.ip);
        let res: Result<Result<(), Error>, Elapsed> = tokio::time::timeout(Duration::from_millis(1000),async move {
            let mut is_writable = false;
            if !self.socket.is_none(){
                is_writable = self.socket.clone().unwrap().lock().await.writable().await.is_ok();
            }

            if !is_writable{
                let _ = self.connect_to_lb().await;
            }
            
            println!("Awaiting connection on {}:{}", self.music_mode_ip, self.music_mode_port);
            let listener = TcpListener::bind(format!("{}:{}",self.music_mode_ip,self.music_mode_port)).await?;

            let _ = self.send_command(format!("{{\"id\":1,\"method\":\"set_music\",\"params\":[0]}}\r\n")).await?;
            sleep(Duration::from_millis(50)).await;
            let _ = self.send_command(format!("{{\"id\":1,\"method\":\"set_music\",\"params\":[1, \"{}\", {}]}}\r\n", self.music_mode_ip.clone(), self.music_mode_port.clone())).await;
            let (socket, addr) = listener.accept().await?;
            println!("Connection recived from {}", addr.ip());
            if !self.socket.is_none(){
                self.socket.clone().unwrap().lock().await.shutdown().await?;
                println!("Origign socket closed");
            }
            self.socket = Some(Arc::new(Mutex::new(socket)));
            self.current_state = LightBulbModes::MusicMode.clone();
            Ok(())
        }).await;
        let ress = match res {
            Ok(ok)=> {
                match ok {
                    Ok(_) => Ok(()),
                    Err(err)=> Err(err)
                }
            },
            Err(err) => Err(Error::new(err))
        };
        ress
    }

    pub async fn switch_connection_mode(&mut self, new_state: LightBulbModes)-> Result<(), Error>{
        match new_state{
            LightBulbModes::MusicMode =>{
                let mut is_connected = false;
                while !is_connected{
                    is_connected = self.startMusicMode().await.is_ok();
                }

            },
            LightBulbModes::StandartMode => {
                self.connect_to_lb().await?;
            },
            LightBulbModes::NoneMode => {
                self.connect_to_lb().await?;
            }
        }

        Ok(())
    }


    //Короче смотри, создаешь поле сокет, в котором либо коннектишься к лампочке, либо инициализиурешь слушанье (благодаря этому можно не ебать мозга и тупо писать туда без разницы music mode это или просто команда)
    pub async fn send_command(&self, command: String) -> Result<(), Error> {  
        let result:Result<Result<(), Error>, Elapsed> = tokio::time::timeout(Duration::from_millis(20), async move {
            // println!("Sending Command to {}", self.ip.clone());
            let str_c1 = self.socket.clone().unwrap();
            let mut stream = str_c1.lock().await;

            let is_writable = stream.writable().await.is_ok();

            if !is_writable {
                return Err(anyhow!("Socket isnt writable"));
            }
            let com = command.as_bytes();
            stream.write_all(com).await?;
            let mut buf: Vec<u8> = vec![0; 4096];
            let _ = stream.read(&mut buf).await?;
            let response_str: String = String::from_utf8(buf)?;
            // println!("\nResponse from ({})", self.ip.clone());
            // println!("{}", response_str);
            Ok(())
        }).await;

        let res: Result<(), Error> = match result {
            Ok(okie)=> match okie {
                Ok(_)=> Ok(()),
                Err(err) => Err(err)
            },
            Err(err)=> Err(err.into())
        };
        res
    }
    pub async fn set_color(&self, color: RGBColor, fader_interval: Duration) {
        let comand: String = format!("{{\"id\":3,\"method\":\"set_rgb\",\"params\":[{}, \"smooth\", {}]}}\r\n", color.get24Bit(), fader_interval.as_millis());
        // println!("{}",comand);
        let _ = self.send_command(comand).await;
    }

    pub async fn set_hsv(&self, hue: u16, sat: u8) {
        let comand: String = format!("{{\"id\":3,\"method\":\"set_hsv\",\"params\":[{}, {},\"smooth\", 100]}}\r\n", hue, sat);
        // println!("{}",comand);
        let _ = self.send_command(comand).await;
    }

    pub async fn set_brightness(&self, bright: u8, fader_interval: Duration){
        let comand: String = format!("{{\"id\":3,\"method\":\"set_bright\",\"params\":[{},\"smooth\", {}]}}\r\n", bright, fader_interval.as_millis());
        // println!("{}",comand);
        let _ = self.send_command(comand).await;
    }

}
pub struct LightBulbProvider {
    lightbulbs: Arc<Mutex<HashMap<String, Arc<Mutex<LightBulb>>>>>,
    socket: Arc<Mutex<UdpSocket>>,
}

pub struct Answer {
    result: bool,
    id: String
}

impl LightBulbProvider {
    pub async fn new() -> Arc<LightBulbProvider> {
        let lbp = LightBulbProvider { 
            lightbulbs: Arc::new(Mutex::new(HashMap::new())),
            socket: Arc::new(Mutex::new(UdpSocket::bind("192.168.0.133:3132").await.expect("msg")))
        };
        
        return Arc::new(lbp);
    }
    
    pub async fn change_connection_mode_for_all(self: Arc<LightBulbProvider>, mode: LightBulbModes){
        let lbs_locked = self.lightbulbs.lock().await;
        let lbs = lbs_locked.clone();
        drop(lbs_locked);
        let keys: Vec<String> = {
            let locked_lbs = self.lightbulbs.lock().await;
            let lightbulbs = locked_lbs.clone();
            drop(locked_lbs);
            lightbulbs.keys().cloned().collect()
        };

        let mut set = JoinSet::new();

        for id in keys {
            let loc_mode = mode.clone();
            println!("Connection mode of {}", &id);
            let selfc = self.clone();
            if(self.clone().lightbulbs.lock().await[&id].lock().await.current_state != loc_mode.clone()){
                println!("LB ({}) is not in music Mode", self.clone().lightbulbs.lock().await[&id].lock().await.ip);
                set.spawn( async move {
                    println!("spawned th");
                    let lbs_locked = selfc.lightbulbs.lock().await;
                    let mut lb_locked = lbs_locked[&id].lock().await;
                    lb_locked.switch_connection_mode(loc_mode).await
                });
            }
        }
        while let Some(res) = set.join_next().await  {
            match res {
                Ok(_)=>println!("Ok"),
                Err(_)=>println!("err")
            }
        }

    }

    pub async fn set_color_for_all(self: Arc<LightBulbProvider>, color: RGBColor, fader_interval: Duration){
        if self.lightbulbs.lock().await.clone().len() <= 0 {
            return ;
        }
        let mut set = JoinSet::new();
        let keys: Vec<String> = {
            self.lightbulbs.lock().await.clone().keys().cloned().collect()
        };

        for id in keys {
            let selfc2 = self.clone();
            set.spawn( async move {
                let lbs = selfc2.lightbulbs.lock().await.clone();
                let lb = lbs[&id].lock().await.clone();
                lb.set_color(color, fader_interval).await;
            });

            while let Some(res) = set.join_next().await  {
                match res {
                    Ok(_)=>print!(""),
                    Err(_)=>print!("")
                }
            }
        }
    }

    pub async fn set_brightness_for_all(self: Arc<LightBulbProvider>, bright: u8,fader_interval: Duration){
        if self.lightbulbs.lock().await.clone().len() <= 0 {
            return ;
        }
        let mut set = JoinSet::new();
        let keys: Vec<String> = {
            self.lightbulbs.lock().await.clone().keys().cloned().collect()
        };

        for id in keys {
            let selfc2 = self.clone();
            set.spawn( async move {
                let lbs = selfc2.lightbulbs.lock().await.clone();
                let lb = lbs[&id].lock().await.clone();
                lb.set_brightness(bright, fader_interval).await;
            });

            while let Some(res) = set.join_next().await  {
                match res {
                    Ok(_)=>print!(""),
                    Err(_)=>print!("")
                }
            }
        }
    }

    pub async fn set_hsv_color_for_all(self: Arc<LightBulbProvider>, hue: u16, sat: u8){
        if self.lightbulbs.lock().await.clone().len() <= 0 {
            return ;
        }
        let mut set = JoinSet::new();
        let keys: Vec<String> = {
            self.lightbulbs.lock().await.clone().keys().cloned().collect()
        };

        for id in keys {
            let selfc2 = self.clone();
            set.spawn( async move {
                let lbs = selfc2.lightbulbs.lock().await.clone();
                let lb = lbs[&id].lock().await.clone();
                lb.set_hsv(hue, sat).await;
            });

            while let Some(res) = set.join_next().await  {
                match res {
                    Ok(_)=>print!(""),
                    Err(_)=>print!("")
                }
            }
        }
    }

    pub async fn discover_routine(self: Arc<LightBulbProvider>) {
        println!("New Round of Routine");
        let _ = self.clone().discover_message_sender().await;

        let selfc = self.clone();
        let _ = tokio::time::timeout(Duration::from_secs(5), async move {
            loop{
                selfc.clone().proceed_lightbulb_answer().await;
            }
        }).await;

        self.clone().check_alive_lbs().await
    }

    pub async fn check_alive_lbs(self: &Arc<LightBulbProvider>){
        let selfc = self.clone();
        let lbsc1 = self.clone().lightbulbs.clone();

        let lbsc2 = lbsc1.lock().await;
        let mut lbs = lbsc2.clone();

        drop(lbsc2);
        drop(lbsc1);
    
        let mut set = JoinSet::new();
        for lb_id in lbs.keys() {
            let lb = Arc::clone(lbs.get(lb_id).unwrap());
            let new_selfc = Arc::clone(&selfc);
            set.spawn(async move {
                return new_selfc.ping_lb(lb.clone()).await
            });
        }
        let mut removed_any: bool = false;
        while let Some(res)= set.join_next().await {
            match res {
                Ok(ok) => match ok {
                    Ok(ans) => {
                        match ans.result {
                            true => {},
                            false => {
                                println!("Lb removed ip: {}", lbs.get(&ans.id).unwrap().lock().await.clone().ip);
                                let _ = &lbs.remove(&ans.id);
                                removed_any = true;
                            },
                        }
                    },
                    Err(err)=> eprintln!("{}", err)
                },
                Err(e) => eprintln!("{}",e)  
            }
        }
        if removed_any{
            let lbsc3 = self.clone().lightbulbs.clone();
            let mut lbsc4 = lbsc3.lock().await;
            *lbsc4 = lbs.clone();
        }

    }
    pub async fn ping_lb(self: &Arc<LightBulbProvider>, lb_unlocked: Arc<Mutex<LightBulb>>) -> Result<Answer, anyhow::Error>{
        let lb = lb_unlocked.clone().lock().await.clone();
        println!("Pinging {}", lb.clone().ip);

        let str_ip = lb.ip.split(':').collect::<Vec<&str>>()[0];

        let ip: IpAddr = IpAddr::V4(Ipv4Addr::from_str(str_ip)?);
        let pinger = AsyncPinger::new();
        let mut set = JoinSet::new();
        
        for _ in 0..4{
            let buffer = Buffer::new();
            
            set.spawn(pinger.send(ip, buffer));
        }

        let mut func_res = 0;
        while let Some(res)= set.join_next().await {
            let result = match res {
                Ok(rtt) => match rtt.result {
                    Ok(_) => true, 
                    Err(_) => false
                },
                Err(_) => false
            };
            if !result {
                func_res += 1;
            }
        }
        let is_alive = if func_res <= 2 {true} else {false}; //Если хотя бы 2 пинга из 4 прошли, то лампочка жива
        println!("LB {} pinged res: {}", lb.ip.clone(), is_alive.clone());

        let ans = Answer {id: lb.id.clone(), result: is_alive.clone() };
        Ok(ans)
    }

    pub async fn discover_message_sender(self: &Arc<LightBulbProvider>) {
        // println!("sending discover msg");
        let selfc = self.clone();
        let socket = selfc.socket.lock().await;
        let ssdp_scan: Vec<u8> = "M-SEARCH * HTTP/1.1\r\nMAN: \"ssdp:discover\"\r\nST: wifi_bulb\r\n".to_string().as_bytes().to_vec();
        let _ = socket.send_to(&ssdp_scan, "239.255.255.250:1982").await;
    }

    pub async fn proceed_lightbulb_answer(self: &Arc<LightBulbProvider>) {
        // println!("Reading lightbulbs answers!)");
        let selfc = self.clone();
        let socket = selfc.socket.lock().await;
        
        let lbs_c1 = selfc.lightbulbs.lock().await;
        let mut lightbulbs = lbs_c1.clone();
        drop(lbs_c1);

        let mut buf: Vec<u8> = vec![0; 4096];
        let (amt, _) = socket.recv_from(&mut buf).await.unwrap();
        let response_str: String = String::from_utf8(buf).unwrap();
        // println!("{}", response_str);
        let mut is_changed = false;
        if amt > 0 {
            let parsed_lb = LightBulb::parse(response_str).await;
            match parsed_lb {
                Ok(mut parsed_lb) => {
                    if !lightbulbs.contains_key(&parsed_lb.id.clone()) {
                        let _ = parsed_lb.init_connection().await;
                        println!("There is New LB! Ip: {}, id: {}", parsed_lb.ip.clone(), parsed_lb.id.clone());
                        lightbulbs.insert(parsed_lb.id.clone(), Arc::new(Mutex::new(parsed_lb)));
                        is_changed = true;
                    }
                },
                Err(error)=> println!("{}", error)
            }
        }
        if is_changed{
            let mut lbs = selfc.lightbulbs.lock().await;
            *lbs = lightbulbs;
        }
    }
}





