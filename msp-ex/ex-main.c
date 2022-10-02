/* --COPYRIGHT--,BSD
 * Copyright (c) 2020, Texas Instruments Incorporated
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * *  Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *
 * *  Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * *  Neither the name of Texas Instruments Incorporated nor the names of
 *    its contributors may be used to endorse or promote products derived
 *    from this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
 * THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR
 * CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
 * EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
 * PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS;
 * OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
 * WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
 * OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
 * EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 * --/COPYRIGHT--*/
// https://dev.ti.com/tirex/explore/node?node=A__AKbG0Wa9znDPo-23SHFFQQ__msp_housekeeping__IOGqZri__LATEST 
//******************************************************************************
//  MSP430FR2433 Demo - I2C I/O Expander
//
//  Description: This example uses the eUSCI module to accept I2C transactions and
//  expand an 8-bit I/O port. The I2C transaction is a 3-byte transaction that
//  accepts a command, group/bit number, and data value to set or read the I/O port.
//  A GUI can be used as the host application or as a monitor for the I2C transactions.
//  I2C slave address is 0x48
//  ACLK = default REFO ~32768Hz
//  SMCLK = MCLK = DCO + FLL + 32KHz REFO REF = 1MHz
//
//                MSP430FR2433
//             ---------------
//         /|\|               |
//          | |               |
//          --|RST       P1.0 |---> BIT0
//            |          P1.1 |---> BIT1
//            |          P1.2 |---> BIT2
//            |          P1.3 |---> BIT3
//UCB0SDA  -->|P1.2/SDA  P1.6 |---> BIT4
//UCB0SCL  -->|P1.3/SCL  P1.7 |---> BIT5
//            |          P2.0 |---> BIT6
//            |          P2.1 |---> BIT7
//
//   Eason Zhou
//   Texas Instruments Inc.
//   Jan 2021
//   Built with Code Composer Studio v9.2 and IAR 7.20
//******************************************************************************
#include <msp430.h>
#include <stdint.h>
#include <stdbool.h>

void Reset_MCU(void);
void Set_All(uint8_t word);
void Set_Group(uint8_t group, uint8_t value);
void Set_Bit(uint8_t bit, uint8_t value);

#define GPIO_ALL        BIT0|BIT1|BIT2|BIT3|BIT4|BIT5|BIT6|BIT7
#define MCLK_FREQ_MHZ 1                                          // MCLK = 1MHz

//Command defines
#define RESET_MCU   0
#define SET_ALL     1
#define SET_GROUP   2
#define SET_BIT     3

//Group defines
#define GROUP_1     BIT4|BIT5|BIT6|BIT7
#define GROUP_2     BIT6|BIT7
#define GROUP_3     BIT0|BIT1

// Declare global variables
volatile bool RxFlag = 0;
unsigned int RxCount = 0;
unsigned char RxData = 0;

uint8_t commandByte, groupbitByte, valueByte;


// main.c
int main(void)
{
    WDTCTL = WDTPW | WDTHOLD;                             // Stop watchdog timer

    __bis_SR_register(SCG0);                                      // disable FLL

    // Initialize Clock System
    // SMCLK = MCLK = DCO + FLL + 32KHz REFO REF = 1MHz
    CSCTL3 |= SELREF__REFOCLK;               // Set REFO as FLL reference source
    CSCTL1 = DCOFTRIMEN | DCOFTRIM0 | DCOFTRIM1 | DCORSEL_0; // DCOFTRIM=3, DCO Range = 1MHz
    CSCTL2 = FLLD_0 + 30;                                       // DCODIV = 1MHz
    __delay_cycles(3);
    __bic_SR_register(SCG0);                                      // enable FLL

    CSCTL4 = SELMS__DCOCLKDIV | SELA__REFOCLK; // set default REFO(~32768Hz) as ACLK source, ACLK = 32768Hz
                                               // default DCODIV as MCLK and SMCLK source
    // Initialize globals
    P1OUT = 0;                                               // Set all pins low
    P1DIR = GPIO_ALL;                                 // Set all pins as outputs

    P1SEL0 |= BIT2 | BIT3;

    P2OUT = 0;                                               // Set all pins low
    P2DIR = GPIO_ALL;                                 // Set all pins as outputs

    P3OUT = 0;                                               // Set all pins low
    P3DIR = GPIO_ALL;                                 // Set all pins as outputs

    P3SEL0 |= BIT1;
    PM5CTL0 &= ~LOCKLPM5; // Disable the GPIO power-on default high-impedance mode
                          // to activate previously configured port settings

    // Configure USCI_B0 for I2C mode
    UCB0CTLW0 = UCSWRST;                      // Software reset enabled
    UCB0CTLW0 |= UCMODE_3 | UCSYNC;           // I2C mode, sync mode
    UCB0I2COA0 = 0x48 | UCOAEN;               // own address is 0x48 + enable
    UCB0CTLW0 &= ~UCSWRST;                    // clear reset register
    UCB0IE |= UCRXIE0;                        // Receive interrupt enable

    __bis_SR_register(LPM0_bits | GIE);            // Go to LPM0 with interrupts
    __no_operation();

    while (1)
    {
        if (RxFlag == 1)
        {
            RxFlag = 0;

            switch (commandByte)
            {
            case RESET_MCU:
                Reset_MCU();
                break;
            case SET_ALL:
                Set_All(valueByte);
                break;
            case SET_GROUP:
                Set_Group(groupbitByte, valueByte);
                break;
            case SET_BIT:
                Set_Bit(groupbitByte, valueByte);
                break;
            default:
                break;
            }
        }
    }
}

#if defined(__TI_COMPILER_VERSION__) || defined(__IAR_SYSTEMS_ICC__)
#pragma vector = USCI_B0_VECTOR
__interrupt void USCIB0_ISR(void)
#elif defined(__GNUC__)
void __attribute__ ((interrupt(USCI_B0_VECTOR))) USCIB0_ISR (void)
#else
#error Compiler not supported!
#endif
{
  switch(__even_in_range(UCB0IV, USCI_I2C_UCBIT9IFG))
  {
    case USCI_NONE: break;                  // Vector 0: No interrupts
    case USCI_I2C_UCALIFG: break;           // Vector 2: ALIFG
    case USCI_I2C_UCNACKIFG: break;         // Vector 4: NACKIFG
    case USCI_I2C_UCSTTIFG: break;          // Vector 6: STTIFG
    case USCI_I2C_UCSTPIFG: break;                // Vector 8: STPIFG
    case USCI_I2C_UCRXIFG3: break;          // Vector 10: RXIFG3
    case USCI_I2C_UCTXIFG3: break;          // Vector 14: TXIFG3
    case USCI_I2C_UCRXIFG2: break;          // Vector 16: RXIFG2
    case USCI_I2C_UCTXIFG2: break;          // Vector 18: TXIFG2
    case USCI_I2C_UCRXIFG1: break;          // Vector 20: RXIFG1
    case USCI_I2C_UCTXIFG1: break;          // Vector 22: TXIFG1
    case USCI_I2C_UCRXIFG0:                 // Vector 24: RXIFG0
        switch (RxCount)
        {
        case 0:
            commandByte = UCB0RXBUF;
            break;
        case 1:
            groupbitByte =  UCB0RXBUF;
            break;
        case 2:
            valueByte =  UCB0RXBUF;
            break;
        }

        RxCount++;

        if (RxCount > 2)
        {
            UCB0IFG &= ~UCRXIFG;
            RxCount = 0;
            RxFlag = 1;
            __bic_SR_register_on_exit(LPM0_bits);   // Exit LPM0
        }
        break;
    case USCI_I2C_UCTXIFG0: break;          // Vector 26: TXIFG0
    case USCI_I2C_UCBCNTIFG: break;         // Vector 28: BCNTIFG
    case USCI_I2C_UCCLTOIFG: break;         // Vector 30: clock low timeout
    case USCI_I2C_UCBIT9IFG: break;         // Vector 32: 9th bit
    default: break;
  }
}

void Reset_MCU(void)
{
    P1OUT &= ~(GPIO_ALL);
    P2OUT &= ~(GPIO_ALL);
}

void Set_All(uint8_t word)
{
    uint8_t temp;

    P1OUT &= ~( GROUP_2);
    P2OUT &= ~(GROUP_1 |GROUP_3);

    temp = (word >> 0) & 1;
    P2OUT |= temp << 4;
    temp = (word >> 1) & 1;
    P2OUT |= temp << 5;
    temp = (word >> 2) & 1;
    P2OUT |= temp << 6;
    temp = (word >> 3) & 1;
    P2OUT |= temp << 7;
    temp = (word >> 4) & 1;
    P1OUT |= temp << 6;
    temp = (word >> 5) & 1;
    P1OUT |= temp << 7;
    temp = (word >> 6) & 1;
    P2OUT |= temp << 0;
    temp = (word >> 7) & 1;
    P2OUT |= temp << 1;
}

void Set_Group(uint8_t group, uint8_t value)
{
    switch (group)
    {
    case 1:
            P2OUT |= (value << 4);
            break;
        case 2:
            P1OUT |= ((value << 6) & 0xC0);
            break;
        case 3:
            P2OUT |= ( value& 0x03);
            break;
        default:
            break;
    }
}

void Set_Bit(uint8_t bit, uint8_t value)
{
    switch (bit)
    {
    case 0:
        if (value)
        {
            P2OUT |= BIT4;
        }
        else
        {
            P2OUT &= ~BIT4;
        }
        break;
    case 1:
        if (value)
        {
            P2OUT |= BIT5;
        }
        else
        {
            P2OUT &= ~BIT5;
        }
        break;
    case 2:
        if (value)
        {
            P2OUT |= BIT6;
        }
        else
        {
            P2OUT &= ~BIT6;
        }
        break;
    case 3:
        if (value)
        {
            P2OUT |= BIT7;
        }
        else
        {
            P2OUT &= ~BIT7;
        }
        break;
    case 4:
        if (value)
        {
            P1OUT |= BIT6;
        }
        else
        {
            P1OUT &= ~BIT6;
        }
        break;
    case 5:
        if (value)
        {
            P1OUT |= BIT7;
        }
        else
        {
            P1OUT &= ~BIT7;
        }
        break;
    case 6:
        if (value)
        {
            P2OUT |= BIT0;
        }
        else
        {
            P2OUT &= ~BIT0;
        }
        break;
    case 7:
        if (value)
        {
            P2OUT |= BIT1;
        }
        else
        {
            P2OUT &= ~BIT1;
        }
        break;
    default:
        break;
    }
}
