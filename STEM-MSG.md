
## 1-wire messaging (STEM INT/MSG)

An UART RXD capable pin from each MCU is used message between MCUs on the Stem.
The pins are all connected together for low bandwidth message exchange.
An UART TXD capable pin can be added to get hardware support for transmitting.
If no TXD pin with hardware support is specified, the RXD pin is used to transmit by using GPIO mode.

Each MCU is required to listen in when they are not sending to track who is the master, and other participants.
Any MCU can send a request to be a master identifying itself using its I2C address.
MCUs can also send messages to notify master of register changes.

- [Hackaday 1-Wire](https://hackaday.com/2015/01/29/easier-uart-to-1-wire-interface/)
- [onewire-over-uart](https://github.com/dword1511/onewire-over-uart)
- [Maxim 1-wire](http://www.maximintegrated.com/app-notes/index.mvp/id/214)
- [CCL & USART & 1-WIRE & MCU communication](https://www.avrfreaks.net/forum/ccl-usart-1-wire-mcu-communication-closedsuccess)
- [UART Intro](https://www.analog.com/en/analog-dialogue/articles/uart-a-hardware-communication-protocol.html)
- [Common 1-Wire review](https://www.engineersgarage.com/what-is-the-1-wire-protocol/)
- [Issue in "Address-Bit Multiprocessor Format" UART](https://e2e.ti.com/support/microcontrollers/msp-low-power-microcontrollers-group/msp430/f/msp-low-power-microcontroller-forum/977034/msp-exp430fr2355-issue-in-address-bit-multiprocessor-format-uart-mode-for-msp430fr2355)

Messages are null-terminated byte strings.


#### Sharing of Message Bus

The main purpose of the bus is for I2C slaves to notify the master when they change state.
The secondary purpose is switching the master MCU.
The primary coordination needs to be between multiple slave MCUs wanting to send notifications.

If an established mechanism can be used great, otherwise one will have to be devised.
The MSP430 supports Idle-Line Multiprocessor Format, perhaps that can be used or extended.
It has 10+ bits idle stop bit between blocks and first character is an address.
The Stem I2C address for the sending device would be used.

To keep multiple senders to clash an MCU will wait for `n` millis before starting transmission
based on its I2C address. This could be `addr % 16 * 2`. During this time it will check to see
if the line goes high or if there is transmission going on. If not it will switch to
transmitting which will drive the line high.

The master could be given the role of sending a special message when the bus will be idle and
a new MCU can send. It would have to save the slaves work in monitoring the bus.

#### numbers

For port input registers the lower 3 bits reflect the port number.
port_no=1 equals P1.

Port number is 3 bits.

Pin index is 3 bits.

Interrupt ID is 7 bits.

Voltage threshold index can be 0 to 2 (2 bits).


#### Message Types

Idle Hint Message

`| address | 0x1 | 0x0 |`


New Master Message

`| address | 0x2 | 0x0 |`


Input Port Changed Message

`| address | 0xA0 + port number | port value | 0x0 |`


Register Changed Message (reserved, not implemented)

`| address | 0xA0 + register | register value | 0x0 |`

Interrupt Received Message

`| address | 0x10 + port number | pin value & pin index | 0x0 |`


Interrupt by ID Message

`| address | 0x20 + port number | interrupt ID | 0x0 |`


Sensor Threshold Message (reserved)

`| address | 0x4 | sensor id or address | byte array value | 0x0 |`


VSOM Voltage Threshold Message

`| address | 0x8 + threshold index | 16 bit voltage level | 0x0 |`


CHARGE Voltage Threshold Message

`| address | 0xA + threshold index | 16 bit voltage level | 0x0 |`



#### stemMSG(baud_rate, rxd_pin[, txd_pin])

This starts to listen for incoming 1-Wire coordination messages other Stem participants

Message
- from address
- event
- param


0x40. VSOM threshold 0 - 2
0x44. CHARGE threshold 0 - 2
0x80. Input pin changed state pin = 4 - 56, 0b10xxxxxx.

