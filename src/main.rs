//! reading temperature from a TMP36 sensor every second    

#![no_std]
#![no_main]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate panic_halt;
extern crate stm32f4xx_hal as hal;

use cortex_m_rt::entry;
use cortex_m::interrupt::{Mutex, free};

use core::ops::DerefMut;
use core::cell::{Cell, RefCell};

use stm32f4::stm32f411::interrupt;

use crate::hal::{
    i2c::I2c, 
    prelude::*, 
    gpio::{gpioa::PA3, Analog},
    stm32,
    delay::Delay,
    adc::{Adc, config::{AdcConfig, SampleTime, Clock, Resolution}},
    timer::{Timer, Event},
    time::Hertz,
    stm32::Interrupt,
    };

use ssd1306::{
    prelude::*, 
    Builder as SSD1306Builder
    };

use embedded_graphics::{
    fonts::{Font12x16, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    style::TextStyleBuilder,
    };

use core::fmt;
use arrayvec::ArrayString;

// globally accessible values
static TEMP_C: Mutex<Cell<i16>> = Mutex::new(Cell::new(0i16));
static TEMP_F: Mutex<Cell<i16>> = Mutex::new(Cell::new(0i16));
static BUF: Mutex<Cell<[u16;5]>> = Mutex::new(Cell::new([0u16;5]));

// interrupt and peripheral for ADC

static TIMER_TIM3: Mutex<RefCell<Option<Timer<stm32::TIM3>>>> = Mutex::new(RefCell::new(None));
static GADC: Mutex<RefCell<Option<Adc<stm32::ADC1>>>> = Mutex::new(RefCell::new(None));
static ANALOG: Mutex<RefCell<Option<PA3<Analog>>>> = Mutex::new(RefCell::new(None));

const FACTOR: f32 = 3300.0/4096.0; //3300 mV / 4096 values for 12-bit ADC

const BOOT_DELAY_MS: u16 = 100; //delay for the I2C to start correctly after power up

#[entry]
fn main() -> ! {
    if let (Some(dp), Some(cp)) = (
        stm32::Peripherals::take(),
        cortex_m::peripheral::Peripherals::take(),
) {
        // Set up the system clock. Speed is not important in this case
        
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(25.mhz()).freeze();
        
        let mut delay = Delay::new(cp.SYST, clocks);
        
        //delay necessary for the I2C to initiate correctly and start on boot without having to reset the board
        delay.delay_ms(BOOT_DELAY_MS);

        //set up ADC
        let gpioa = dp.GPIOA.split();
        let adcconfig = AdcConfig::default().clock(Clock::Pclk2_div_8).resolution(Resolution::Twelve);
        let adc = Adc::adc1(dp.ADC1, true, adcconfig);
                
        let pa3 = gpioa.pa3.into_analog();

        // move the PA3 pin and the ADC into the 'global storage'
        free(|cs| {
            *GADC.borrow(cs).borrow_mut() = Some(adc);        
            *ANALOG.borrow(cs).borrow_mut() = Some(pa3);            
        });


        // Set up I2C - SCL is PB8 and SDA is PB9; they are set to Alternate Function 4
        let gpiob = dp.GPIOB.split();
        let scl = gpiob.pb8.into_alternate_af4().set_open_drain();
        let sda = gpiob.pb9.into_alternate_af4().set_open_drain();
        let i2c = I2c::i2c1(dp.I2C1, (scl, sda), 400.khz(), clocks);

        // Set up the display
        let mut disp: GraphicsMode<_> = SSD1306Builder::new().size(DisplaySize::Display128x32).connect_i2c(i2c).into();
        disp.init().unwrap();

        // set up timer and interrupts
        let mut adctimer = Timer::tim3(dp.TIM3, Hertz(1), clocks); //adc update every 1 s
        adctimer.listen(Event::TimeOut);
                
        free(|cs| {
            TIMER_TIM3.borrow(cs).replace(Some(adctimer));
            });

        let mut nvic = cp.NVIC;
            unsafe {            
                nvic.set_priority(Interrupt::TIM3, 1);
                cortex_m::peripheral::NVIC::unmask(Interrupt::TIM3);
            }
                        
            cortex_m::peripheral::NVIC::unpend(Interrupt::TIM3);

        //set up text for the display
        let text_style = TextStyleBuilder::new(Font12x16).text_color(BinaryColor::On).build();
        

        loop {
                        
            let mut buf_temp_c = ArrayString::<[u8; 7]>::new(); //buffer for the temperature reading
            let mut buf_temp_f = ArrayString::<[u8; 7]>::new(); //buffer for the temperature reading
        
            //clean up the display    
            for x in 0..72 {
                for y in 0..32 {
                    disp.set_pixel(x,y,0);
                }
            }

            let celsius = free(|cs| TEMP_C.borrow(cs).get()); //get the current temperature in Celsius
            let fahrenheit = free(|cs| TEMP_F.borrow(cs).get()); //get the current temperature in Fahrenheit
            
            formatter(&mut buf_temp_c, celsius, 67 as char); // 67 is "C" in ASCII

            Text::new(buf_temp_c.as_str(), Point::new(0, 0)).into_styled(text_style).draw(&mut disp);

            formatter(&mut buf_temp_f, fahrenheit, 70 as char); // 70 is "F" in ASCII

            Text::new(buf_temp_f.as_str(), Point::new(0, 16)).into_styled(text_style).draw(&mut disp);

            disp.flush().unwrap();
            
            delay.delay_ms(200_u16);
            
            }

    }

    loop {}
}


#[interrupt]

// TEMP get updated every time the interrupt fires 
// read from ADC on pins PA3 and PA4

fn TIM3() {
        
    free(|cs| {
        stm32::NVIC::unpend(Interrupt::TIM3);
        if let (Some(ref mut tim3), Some(ref mut adc), Some(ref mut analog)) = (
        TIMER_TIM3.borrow(cs).borrow_mut().deref_mut(),
        GADC.borrow(cs).borrow_mut().deref_mut(),
        
        ANALOG.borrow(cs).borrow_mut().deref_mut())
        
        {
            tim3.clear_interrupt(Event::TimeOut);
            let sample = adc.convert(analog, SampleTime::Cycles_480);

            let mut buf = BUF.borrow(cs).get(); //get the current buffer

            let new_buf = circular(&buf, sample); //buffer update

            BUF.borrow(cs).replace(new_buf); //update the global buffer

            let avg_sample = average(&new_buf);
            
            let voltage = avg_sample * FACTOR; //ADC reading converted to milivolts

            //the common formula is (milivolts - 500) / 10
            //10mV per Celsius degree with 500 mV offset
            //
            //as we want to get the tenths of the degree and display them easily
            //we multiply the result by 10

            let celsius = (voltage - 500.0) / 10.0; 

            let mut fahrenheit = celsius * 9.0;
            fahrenheit /= 5.0;
            fahrenheit += 32.0;

            //as we want to get the tenths of the degree and display them easily
            //we multiply the results by 10

            TEMP_C.borrow(cs).replace((celsius * 10.0) as i16);
            TEMP_F.borrow(cs).replace((fahrenheit * 10.0) as i16);
        }
    });
}




fn formatter(buf: &mut ArrayString<[u8; 7]>, val: i16, unit: char) {   
    // helper function for the display    
    // takes a mutable text buffer, value and unit symbol as arguments
    // default sign is + (43 in ASCII)
    // in order to correctly handle negative values, their sign has to be reversed before splitting into digits
    let mut sign: char = 43 as char; 
    
    if val < 0 {
        sign = 45 as char;
    };
    
    let mut new_val = val;
    if val < 0 {
        new_val *= -1; 
    }

    let tenths = new_val%10;
    let singles = (new_val/10)%10;
    let tens = (new_val/100)%10;
                
    fmt::write(buf, format_args!("{}{}{}.{} {}", sign, tens as u8, singles as u8, tenths as u8, unit)).unwrap();

}


fn circular(buf: &[u16;5], val: u16) -> [u16;5] {

    //simple circular buffer, first in first out
    let mut new_buf: [u16;5] = [0u16;5];
        
    for i in 0..4 {
        new_buf[i] = buf[i+1];
    }
    new_buf[4] = val;

    return new_buf
}

fn average(buf: &[u16;5]) -> f32 {
    //returns an average value of the buffer
    let mut total: u16 = 0u16;
    for i in buf.iter() {
        total += i;
    }
    return total as f32 / 5.0
}