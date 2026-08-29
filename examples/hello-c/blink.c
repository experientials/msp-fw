#include <msp430.h>
/* Minimal FR2476 hello-world: blink P1.0. Verifies the msp430-gcc toolchain. */
int main(void) {
    WDTCTL = WDTPW | WDTHOLD;      /* stop watchdog */
    PM5CTL0 &= ~LOCKLPM5;         /* FR2xx: unlock GPIO after power-up */
    P1DIR |= BIT0;
    for (;;) {
        P1OUT ^= BIT0;
        __delay_cycles(200000);
    }
}
