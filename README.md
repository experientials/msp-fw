# MSP430 I2C Firmware

The firmware must provide the following features

- I/O Expander PCA9698-like API exposed on I2C (Stem)
- Event driven runtime using interrupts
- Reset/Init setting up the firmware runtime
- Docker build container Debian based that runs the build script
- I can modify and recompile the init/configuration with a common code editor
- Written in C, C++ or Rust

You will create a GitHub repo for building and flashing firmware on a MSP430FR2476 evaluation board.
The build will output a binary image and be able to also flash the development board via USB.
The build is made to run from a shell on Unix.

Init function can configure
- The USI pins used for I2C port to listen on
- The I2C address to respond to
- Initial register values
- Register callback for reading custom register
- Pins for Stem 1-Wire Messaging

The firmware will ultimately be written in Rust using [msp430_rt](https://docs.rs/msp430-rt/0.2.4/msp430_rt/),
with the initial version potentially being written in C or C++.

- [MSP430 timer sample code](https://embedded.fm/blog/ese101-msp430-timer-example)
- [Voltage monitor sample code](https://training.ti.com/msp430-housekeeping-voltage-monitor)
- [ADC wake and transmit on threshold](https://training.ti.com/msp-mcu-training-adc-wake-and-transmit-english?context=1147398-1147608-1147442)
- [Alternative I2C lib](https://github.com/jwr/msp430_usi_i2c)
- [MSP430 I2C Concepts](https://dev.ti.com/tirex/explore/node?node=A__AN8bTD5cSL1aI5MBrThs3A__com.ti.MSP430_ACADEMY__bo90bso__LATEST)
- [Decoding a Two-Wire SPI-esque Serial Protocol](https://electronics.stackexchange.com/questions/325075/decoding-a-two-wire-spi-esque-serial-protocol)


:[Hardware](HARDWARE.md)

:[Firmware API](FIRMWARE-API.md)

:[I2C API](I2C-API.md)

:[Linux Device Support](LINUX-SUPPORT.md)

:[Stem MSG](STEM-MSG.md)


## Upwork task

MSP430 PCA9698 compatible I/O Expander, Voltage meter and I2C Proxy

The project is split in 3 milestones. First a basic I/O Expander, Second Voltage metering and Interrupts, 
Thirdly Messaging with 1-Wire UART logic. The firmware must build in on Raspberry Pi using CD/CI.
Texas Instruments has a demo App for Voltage Metering and I/O Expander.
A simple hardware test setup that runs a Python Unit Test on the Raspberry Pi must show it to be working.

You should already have existing MSP430 LaunchPad and Raspberry Pi boards to work with.
I can send you specific boards, but I will need them back at the end of the project.
I expect a robust initial implementation with room for further development.

Later enhancements will include voltage monitoring, multi-MCU coordination, event messaging, slave sensors and rewriting in Rust.

Tests are run on the Raspberry Pi. Python would be a good choice.
They could be written as Python unit tests that logs progress to the console.

Once you are ready to work on the project I will invite you to collaborate on a GitHub repository, where all code and notes 
should be kept.

Show past work

- Show past work on tweaking I2C slaves and UART logic in MCU firmware
- Show past work of low power, async, event/timer driven MCU firmware
- How would you build a firmware in Rust and Test it with a Raspberry Pi?
- Show past experience with Device Drivers and Apps on Zephyr RTOS
- Show past experience with MSP430 firmware
- Show past work with implementing I2C slave firmware
- Show past work measuring voltage
- Show past experience with Rust on MCUs
- Have you worked with GitHub and GitHub actions before?


## Milestone 1

Implement firmware that supports input and output registers and pins along the lines of the PCA9698 API.
Support Device ID I2C call.
No support for SMBAlert, GPIO All Call.

Verify the implementation in hardware by running test scripts on Raspberry Pi.
How many changes can it handle?
Provide firmware source/binary in Git repo.
Provide test script and setup in Git repo.
Provide build setup in Git repo.


## Milestone 2

Support reading voltage levels as VSOM and CHANGE registers.

Trigger interrupt pin when port input changes like PCA9698.
Queue port change event/message when input changes.
Update test script to saturate interrupt triggering.
How many interrupts can it handle?


## Milestone 3

STEM MSG is monitored to determine when sending is allowed.
Implement sending STEM MSG from the event queue when STEM MSG is free.

Implement Voltage threshold monitor.

Test it with two MSP430 attached. They should compete to send notifications to the master.
The Raspberry Pi is the master.
How many interrupt notifications can it handle?


## Milestone 4

Implement Zephyr RTOS demo application using standard Device Drivers to I/O with the MSP430 using the Stem I2C API.

Implement test script to run on Zephyr nRF52832 or 52840 which saturates the I2C API with read/write calls.
Trigger interrupts, input and output GPIO pins on the MSP430.

Implement GitHub Action to run tests on a Raspberry Pi build slave.


## Milestone 5

Support the GPIO All Call functionality

Support SMBAlert command.

Support persisted address & register states

Support update the init code using an I2C command


