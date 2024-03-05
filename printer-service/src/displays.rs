use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Dimensions,
    pixelcolor::{Rgb565, RgbColor},
};
use embedded_graphics_framebuf::FrameBuf;
use mipidsi::Builder;
use rppal::{
    gpio::{Gpio, OutputPin},
    hal::Delay,
    spi::{Bus, Mode, SlaveSelect, Spi},
};

const DC0: u8 = 22;
const BL0: u8 = 19;
const RST0: u8 = 27;

const DC1: u8 = 4;
const BL1: u8 = 13;
const RST1: u8 = 24;

const DC2: u8 = 5;
const BL2: u8 = 12;
const RST2: u8 = 23;

// Display
const W0: usize = 240;
const H0: usize = 240;

const W1: usize = 160;
const H1: usize = 80;

pub const BLACK: Rgb565 = Rgb565::new(5, 10, 6);

pub struct Displays {
    pub left: FrameBuf<Rgb565, [Rgb565; W1 * H1]>,
    pub center: FrameBuf<Rgb565, [Rgb565; W0 * H0]>,
    pub right: FrameBuf<Rgb565, [Rgb565; W1 * H1]>,
    d_left: mipidsi::Display<
        SPIInterfaceNoCS<Spi, rppal::gpio::OutputPin>,
        mipidsi::models::ST7735s,
        rppal::gpio::OutputPin,
    >,
    d_center: mipidsi::Display<
        SPIInterfaceNoCS<Spi, rppal::gpio::OutputPin>,
        mipidsi::models::ST7789,
        rppal::gpio::OutputPin,
    >,
    d_right: mipidsi::Display<
        SPIInterfaceNoCS<Spi, rppal::gpio::OutputPin>,
        mipidsi::models::ST7735s,
        rppal::gpio::OutputPin,
    >,
    bl_left: OutputPin,
    bl_center: OutputPin,
    bl_right: OutputPin,
}

impl Displays {
    pub fn new() -> Self {
        // GPIO
        let gpio = Gpio::new().unwrap();

        let dc0 = gpio.get(DC0).unwrap().into_output();
        let mut bl_center = gpio.get(BL0).unwrap().into_output();
        let rst0 = gpio.get(RST0).unwrap().into_output();

        let dc1 = gpio.get(DC1).unwrap().into_output();
        let mut bl_right = gpio.get(BL1).unwrap().into_output();
        let rst1 = gpio.get(RST1).unwrap().into_output();

        let dc2 = gpio.get(DC2).unwrap().into_output();
        let mut bl_left = gpio.get(BL2).unwrap().into_output();
        let rst2 = gpio.get(RST2).unwrap().into_output();

        let spi0 = Spi::new(Bus::Spi1, SlaveSelect::Ss0, 10000000_u32, Mode::Mode0).unwrap();
        let di0 = SPIInterfaceNoCS::new(spi0, dc0);
        let mut delay = Delay::new();
        let d_center = Builder::st7789(di0)
            // width and height are switched on purpose because of the orientation
            .with_display_size(H0 as u16, W0 as u16)
            // this orientation applies for the Display HAT Mini by Pimoroni
            .with_orientation(mipidsi::Orientation::Portrait(false))
            .with_invert_colors(mipidsi::ColorInversion::Inverted)
            .init(&mut delay, Some(rst0))
            .unwrap();

        let spi1 = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 10000000_u32, Mode::Mode0).unwrap();
        let di1 = SPIInterfaceNoCS::new(spi1, dc1);
        let d_right = Builder::st7735s(di1)
            // width and height are switched on purpose because of the orientation
            .with_display_size(H1 as u16, W1 as u16)
            // this orientation applies for the Display HAT Mini by Pimoroni
            .with_orientation(mipidsi::Orientation::PortraitInverted(false))
            .with_invert_colors(mipidsi::ColorInversion::Inverted)
            .with_color_order(mipidsi::ColorOrder::Bgr)
            .with_window_offset_handler(|_| (26, 0))
            .init(&mut delay, Some(rst1))
            .unwrap();

        let spi2 = Spi::new(Bus::Spi0, SlaveSelect::Ss1, 10000000_u32, Mode::Mode0).unwrap();
        let di2 = SPIInterfaceNoCS::new(spi2, dc2);
        let d_left = Builder::st7735s(di2)
            // width and height are switched on purpose because of the orientation
            .with_display_size(H1 as u16, W1 as u16)
            // this orientation applies for the Display HAT Mini by Pimoroni
            .with_orientation(mipidsi::Orientation::PortraitInverted(false))
            .with_invert_colors(mipidsi::ColorInversion::Inverted)
            .with_color_order(mipidsi::ColorOrder::Bgr)
            .with_window_offset_handler(|_| (26, 0))
            .init(&mut delay, Some(rst2))
            .unwrap();

        let fbuf_left_data = [BLACK; W1 * H1];
        let left = FrameBuf::new(fbuf_left_data, H1, W1);
        let fbuf_right_data = [BLACK; W1 * H1];
        let right = FrameBuf::new(fbuf_right_data, H1, W1);
        let fbuf_center_data = [BLACK; W0 * H0];
        let center = FrameBuf::new(fbuf_center_data, H0, W0);

        bl_center.set_high();
        bl_left.set_high();
        bl_right.set_high();

        let mut displays = Self {
            left,
            center,
            right,
            d_center,
            d_left,
            d_right,
            bl_left,
            bl_center,
            bl_right,
        };
        displays.flush_to_displays();
        displays
    }

    pub fn flush_to_displays(&mut self) {
        self.d_left
            .fill_contiguous(&self.left.bounding_box(), self.left.data)
            .unwrap();
        self.d_center
            .fill_contiguous(&self.center.bounding_box(), self.center.data)
            .unwrap();
        self.d_right
            .fill_contiguous(&self.right.bounding_box(), self.right.data)
            .unwrap();
    }
}

impl Drop for Displays {
    fn drop(&mut self) {
        // Turn off backlight and clear the display
        self.bl_left.set_low();
        self.d_left.clear(Rgb565::BLACK).unwrap();

        self.bl_right.set_low();
        self.d_right.clear(Rgb565::BLACK).unwrap();

        self.bl_center.set_low();
        self.d_center.clear(Rgb565::BLACK).unwrap();
    }
}
