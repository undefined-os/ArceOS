# app name
APP ?=
ELF ?=
# variables for build
RUSTFLAGS := "-C link-arg=-no-pie -C link-arg=-T$(shell pwd)/link.lds"
BUILD_TARGET := x86_64-unknown-hermit
BUILD_FLAGS := --target $(BUILD_TARGET) -Zbuild-std=std,panic_abort

# variables for run
QEMU := qemu-system-x86_64
QEMU_FLAGS := -m 128M -smp 1 -machine q35 -nographic -kernel
QEMU_TARGET := ../../target/$(BUILD_TARGET)/debug/$(ELF)

.PHONY: all build run clean

# default target
all: build run

# build hermit app
build:
	@echo "Building $(APP) for target$(BUILD_TARGET)..."

	cd $(APP) \
	&& RUSTFLAGS=$(RUSTFLAGS) AX_LINK=$(shell pwd) cargo build $(BUILD_FLAGS) \
	&& cd ..

# run hermit app in qemu
run:
	@echo "Running $(APP) with QEMU..."
	$(QEMU) $(QEMU_FLAGS) $(QEMU_TARGET)

# clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean