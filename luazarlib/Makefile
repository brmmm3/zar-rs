LUA_PKG ?= lua5.4
LUA ?= lua5.4
CC ?= cc
PKG_CONFIG ?= pkg-config
CFLAGS ?= -O2
CZARLIB_DIR ?= ../target/debug

.PHONY: all test clean

all: build/_luazarlib.so

build/_luazarlib.so: src/luazarlib.c ../czarlib/include/czarlib.h
	@mkdir -p build
	$(CC) $(CFLAGS) -std=c11 -Wall -Wextra -Werror -fPIC -shared \
		$(shell $(PKG_CONFIG) --cflags $(LUA_PKG)) -I../czarlib/include $< \
		-L$(CZARLIB_DIR) -lczarlib $(shell $(PKG_CONFIG) --libs $(LUA_PKG)) -o $@

test: all
	LUA_PATH='./?.lua;;' LUA_CPATH='./build/?.so;;' \
	LD_LIBRARY_PATH="$(abspath $(CZARLIB_DIR)):$${LD_LIBRARY_PATH:-}" $(LUA) tests/test.lua

clean:
	rm -f build/_luazarlib.so
