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
    // SAFETY: The Pio is kept alive by main's infinite loop and never dropped.
    let mut pio_owned = Pio::new(p.PIO0, Irqs);
    let pio: &'static mut Pio<'static, PIO0> = unsafe {
        core::mem::transmute(&mut pio_owned)
    };
    
    // To prevent pio_owned from being dropped, we deliberately leak it
    core::mem::forget(pio_owned);

    // Create PIO pins from GPIO pins
    let clk_pin = pio.common.make_pio_pin(p.PIN_2);
    let mosi_pin = pio.common.make_pio_pin(p.PIN_3);
    let miso_pin = pio.common.make_pio_pin(p.PIN_4);

    // Create SPI configuration
    let config = SpiMasterConfig {
        clk_div: 8, // Clock divider for SPI clock rate
    };

    // Create SPI master
    let mut spi = PioSpiMaster::new(
        pio,
        &clk_pin,
        &mosi_pin,
        &miso_pin,
        config,
    );

    loop {
        info!("Running");
        
        // Example: Send 50-bit message with read flag set
        // Message format:
        // - Bits [49:0]: 50-bit data to transmit
        // - Bit 50: read flag (1 = read 50-bit response, 0 = write only)
        let msg = 0x0000000001234567_89 | (1 << 50);
        
        if let Some(response) = spi.transfer(msg) {
            info!("Received: 0x{:012x}", response);
        } else {
            info!("No response (read flag not set)");
        }

        Timer::after_millis(1000).await;
    }
}
