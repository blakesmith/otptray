APPLICATION=otptray
VERSION=0.0.1
PACKAGE_DIR=$(APPLICATION)_$(VERSION)_amd64
DEB_DIR=deb
DEB=$(PACKAGE_DIR).deb
BIN=target/release/$(APPLICATION)

RUST_SOURCES := $(shell find src -name '*.rs')

all: $(DEB)

$(BIN): $(RUST_SOURCES)
	cargo build --release

$(PACKAGE_DIR): $(BIN)
	mkdir -p $(PACKAGE_DIR)/DEBIAN
	mkdir -p $(PACKAGE_DIR)/usr/bin
	cp $(DEB_DIR)/control $(PACKAGE_DIR)/DEBIAN
	cp $(BIN) $(PACKAGE_DIR)/usr/bin
	cp -rv share $(PACKAGE_DIR)/usr/share

$(DEB): $(PACKAGE_DIR)
	dpkg-deb --build $(PACKAGE_DIR)

clean:
	rm -rf $(PACKAGE_DIR) $(DEB)

deep_clean: clean
	cargo clean

.PHONY: clean deep_clean all
