use anyhow::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{result::Result::Ok, time::{Duration, self}, sync::Arc, collections::HashMap, net::{IpAddr, Ipv4Addr}, str::FromStr, future};



use std::time::{SystemTime, UNIX_EPOCH};

const FF_PASS_MAX: u8 = 1;
const FF_PASS_MIN: u8 = 2;

pub struct VolAnalyzer {
    _dt: Duration,
    _ampliDt: Duration,
    _window: i32,
    _tmrPulse: time::SystemTime,
    _tmrDt: time::SystemTime,
    _tmrAmpli: time::SystemTime,
    _raw: i32,
    _rawMax: i32,
    _max: i32,
    _maxs: i32,
    _mins: i32,
    count: i32,
    pub _volMin: i32,
    pub _volMax: i32,
    pub _trsh: i32,
    pub _pulseTrsh: i32,
    pub _pulseMin: i32,
    pub _pulseTout: Duration,
    
    _pulse: bool,
    _pulseState: bool,
    _first: bool,

    minF: FastFilterVA,
    maxF: FastFilterVA,
    volF: FastFilterVA,
    // int _dt = 500;      // 500 мкс между сэмплами достаточно для музыки
    // int _ampliDt = 150; // сглаживание амплитудных огибающих
    // int _window = 20;   // при таком размере окна получаем длительность оцифровки 10 мс
    
    // int _pin;
    // uint32_t _tmrPulse = 0, _tmrDt = 0, _tmrAmpli = 0;
    // int _raw = 0, _rawMax = 0;
    // int _max = 0, _maxs = 0, _mins = 1023;
    // int count = 0;
    // int _volMin = 0, _volMax = 100, _trsh = 0;
    // int _pulseTrsh = 80, _pulseMin = 0, _pulseTout = 100;
    // bool _pulse = 0, _pulseState = 0, _first = 0;

    // FastFilterVA minF, maxF, volF;
}

impl VolAnalyzer {
    pub fn new() -> VolAnalyzer{
        let mut analyz  = VolAnalyzer {
            _dt: Duration::from_micros(100),
            _ampliDt: Duration::from_millis(125),
            _window: 20,
            _tmrPulse: time::SystemTime::now(),
            _tmrDt: time::SystemTime::now(),
            _tmrAmpli: time::SystemTime::now(),
            _raw: 0,
            _rawMax: 0,
            _max: 0,
            _maxs: 0,
            _mins: 1023,
            count: 0,
            _volMin:0,
            _volMax: 100,
            _trsh: 0,
            _pulseTrsh: 80,
            _pulseMin: 0,
            _pulseTout: Duration::from_millis(100),
            _pulse: false,
            _pulseState: false,
            _first: false,
            maxF: FastFilterVA::new(None, None),
            minF:FastFilterVA::new(None, None),
            volF: FastFilterVA::new(None, None)
        };
        analyz.volF.set_dt(20);
        analyz.volF.set_pass(FF_PASS_MAX);
        analyz.maxF.set_pass(FF_PASS_MAX);
        analyz.minF.set_pass(FF_PASS_MIN);
        analyz.set_vol_k(25);
        analyz.set_ampli_k(30);

        analyz
    }

    pub fn tick(&mut self, this_read: i32) -> bool{
        self.volF.compute();
        if time::SystemTime::now().duration_since(self._tmrAmpli).unwrap() >= self._ampliDt {
            self._tmrAmpli = time::SystemTime::now();
            self.maxF.set_raw(self._maxs);
            self.minF.set_raw(self._mins);
            self.maxF.compute();
            self.minF.compute();
            self._maxs = 0;
            self._mins = 1023;
        }
        if time::SystemTime::now().duration_since(self._tmrDt).unwrap()  >= self._dt{
            self._tmrDt = time::SystemTime::now();
            if this_read > self._max { self._max = this_read; }
            
            if !self._first {
                self._first = true;
                self.maxF.set_fil(this_read);
                self.minF.set_fil(this_read);
            }
            
            self.count += 1;
            if self.count >= self._window {
                self._raw = self._max;
                if self._max > self._maxs { self._maxs = self._max; }
                if self._max < self._mins { self._mins = self._max; }
                self._rawMax = self._maxs;
                if self.get_max() - self.get_min() < self._trsh {self._max = 0;} // если окно громкости меньше порога, то 0
                self._max = Self::constrain(Self::map(self._max, self.get_min(), self.get_max(), self._volMin, self._volMax), self._volMin, self._volMax);
                self.volF.set_raw(self._max);

                if !self._pulseState {
                    if self._max <= self._pulseMin && time::SystemTime::now().duration_since(self._tmrPulse).unwrap() >= self._pulseTout {self._pulseState = true}
                } else {
                    if self._max > self._pulseTrsh {
                        self._pulseState = false;
                        self._pulse = true;
                        self._tmrPulse = time::SystemTime::now();

                    }
                }

                self._max = 0;
                self.count = 0;
                return true;
            }
        }

        return false;
    }

    
    pub fn get_vol(&self) -> i32 { 
        return self.volF.get_fil();
    }

    pub fn get_pulse(&mut self)-> bool{
        if(self._pulse){
            self._pulse = false;
            return true;
        }
        return false;
    }

    pub fn map(value: i32, from_min: i32, from_max: i32, to_min: i32, to_max: i32) -> i32 {
        // REWRITE WITH CHECKED SUB/DIV/MULT etc
        
        // ORIGIANL Calculation:
        // ((value - from_min) * (to_max - to_min) / (from_max - from_min)) + to_min  
        
        let val_sub_from_min: i32 = value.checked_sub(from_min).unwrap_or(i32::MIN);
        let tomax_sub_tomin = to_max.checked_sub(to_min).unwrap_or(i32::MIN);
        let frommax_sub_frommin = from_max.checked_sub(from_min).unwrap_or(i32::MIN);
        let checked_mult: i32 = val_sub_from_min.checked_mul(tomax_sub_tomin).unwrap_or(i32::MAX);
        let checked_divis: i32 = checked_mult.checked_div(frommax_sub_frommin).unwrap_or(i32::MAX); 
        checked_divis.checked_add(to_min).unwrap_or(i32::MAX)
    }


    pub fn constrain<T: Ord>(value: T, min_value: T, max_value: T) -> T {
        if value < min_value {
            min_value
        } else if value > max_value {
            max_value
        } else {
            value
        }
    }

    pub fn get_min(&self)-> i32{
        self.minF.get_fil()
    }
    pub fn get_max(&self) -> i32{
        self.maxF.get_fil()
    }  

    pub fn setDt(&mut self,dt: Duration){
        self._dt = dt;
    }

    pub fn set_vol_k(&mut self, k: u8){
        self.volF.set_k(k);
    }
    pub fn set_ampli_k(&mut self, k: u8){
        self.maxF.set_k(k);
        self.minF.set_k(k);
    }   

}

pub struct FastFilterVA {
    _tmr: u64,
    _dt: i32,
    _k1: u8, 
    _k2: u8, 
    _pass: u8,
    _raw_f: i32, 
    _raw: i32,
}
impl FastFilterVA {
    // коэффициент 0-31
    fn new(k: Option<u8>, dt: Option<i32>) -> Self {
        let mut obj = FastFilterVA {
            _tmr: 0,
            _dt: 0,
            _k1: 20,
            _k2: 12,
            _pass: 0,
            _raw_f: 0,
            _raw: 0,
        };
        obj.set_k(k.unwrap_or(20));
        obj.set_dt(dt.unwrap_or(0));
        obj
    }
    
    // коэффициент 0-31
    fn set_k(&mut self, k: u8) {
        self._k1 = k;
        self._k2 = k / 2;
    }
    
    // установить период фильтрации
    fn set_dt(&mut self, dt: i32) {
        self._dt = dt;
    }
    
    // установить режим пропуска (FF_PASS_MAX / FF_PASS_MIN)
    fn set_pass(&mut self, pass: u8) {
        self._pass = pass;
    }
    
    // установить исходное значение для фильтрации
    fn set_raw(&mut self, raw: i32) {
        self._raw = raw;
    }
    
    // установить фильтрованное значение
    fn set_fil(&mut self, fil: i32) {
        self._raw_f = fil;
    }
    
    // расчёт по таймеру
    fn compute(&mut self) {
        if self._dt == 0 || self.get_millis() - self._tmr >= self._dt as u64 {
            self._tmr = self.get_millis();
            self.compute_now();
        }
    }
    
    // произвести расчёт сейчас
    fn compute_now(&mut self) {
        let mut k = self._k1;
        if (self._pass == FF_PASS_MAX && self._raw > self._raw_f) || (self._pass == FF_PASS_MIN && self._raw < self._raw_f) {
            k=self._k2
        }
        let prom: i32 = (k as i32) * self._raw_f + ((32_i32 - (k as i32)) as i32) * self._raw;
        self._raw_f = prom >> 5;
    }
    
    // получить фильтрованное значение
    fn get_fil(&self) -> i32 {
        self._raw_f
    }
    
    // получить последнее сырое значение
    fn get_raw(&self) -> i32 {
        self._raw
    }
    
    fn get_millis(&self) -> u64 {
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
        since_the_epoch.as_millis() as u64
    }

}
