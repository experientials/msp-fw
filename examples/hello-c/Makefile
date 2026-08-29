# hello-c — minimal FR2476 blink. Built inside the MSP430 toolchain container,
# where msp430-elf-gcc and $MSP430_TOOLCHAIN_PATH are already set. Run via
# `scripts/build.sh hello-c` (wraps docker) or `make` from inside the container.
DEVICE  := msp430fr2476
CC      := msp430-elf-gcc
SIZE    := msp430-elf-size
INC     := $(MSP430_TOOLCHAIN_PATH)/include

CFLAGS  := -mmcu=$(DEVICE) -I$(INC) -Os -Wall -Wextra -std=c11 \
           -ffunction-sections -fdata-sections
LDFLAGS := -L$(INC) -Wl,--gc-sections

all: build/blink.elf

build:
	mkdir -p build

build/blink.elf: blink.c | build
	$(CC) $(CFLAGS) $< -o $@ $(LDFLAGS)
	$(SIZE) $@

clean:
	rm -rf build

.PHONY: all clean
