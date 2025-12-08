#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::Pio;
use embassy_rp::bind_interrupts;
use embassy_time::Timer;
use pio_spi::{PioSpiMaster, SpiMasterConfig};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("PIO SPI Example Starting");

    let p = embassy_rp::init(Default::default());

    // Initialize PIO with pins
    let mut pio = Pio::new(p.PIO0, Irqs);

    // Create PIO pins from GPIO pins
    let clk_pin = pio.common.make_pio_pin(p.PIN_2);
    let mosi_pin = pio.common.make_pio_pin(p.PIN_3);
    let miso_pin = pio.common.make_pio_pin(p.PIN_4);

    // Demo 1: 16-bit transfer
    {
        info!("=== 16-bit Transfer Demo ===");
        let config = SpiMasterConfig {
            clk_div: 8,
            message_size: 16,
        };

        let mut spi = PioSpiMaster::new(
            &mut pio,
            &clk_pin,
            &mosi_pin,
            &miso_pin,
            config,
        );

        let data = 0xABCD_u16 as u64;
        info!("Sending: 0x{:04x}", data);
        let response = spi.transfer(data);
        info!("Received: 0x{:04x}", response & 0xFFFF);
        Timer::after_millis(100).await;
    }

    // Demo 2: 50-bit transfer
    {
        info!("=== 50-bit Transfer Demo ===");
        let config = SpiMasterConfig {
            clk_div: 8,
            message_size: 50,
        };

        let mut spi = PioSpiMaster::new(
            &mut pio,
            &clk_pin,
            &mosi_pin,
            &miso_pin,
            config,
        );

        let data = 0x0000000001234567_89u64;
        info!("Sending: 0x{:012x}", data);
        let response = spi.transfer(data);
        info!("Received: 0x{:012x}", response);
        Timer::after_millis(100).await;
    }

    // Demo 3: 60-bit transfer
    {
        info!("=== 60-bit Transfer Demo ===");
        let config = SpiMasterConfig {
            clk_div: 8,
            message_size: 60,
        };

        let mut spi = PioSpiMaster::new(
            &mut pio,
            &clk_pin,
            &mosi_pin,
            &miso_pin,
            config,
        );

        let data = 0x0FEDCBA987654321_u64;
        info!("Sending: 0x{:015x}", data);
        let response = spi.transfer(data);
        info!("Received: 0x{:015x}", response);
        Timer::after_millis(100).await;
    }

    info!("Demo complete");
    loop {
        Timer::after_millis(1000).await;
    }
}
