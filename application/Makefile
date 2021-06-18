APPLICATION=otptray
VERSION=0.0.1
PACKAGE_DIR=$(APPLICATION)_$(VERSION)_amd64
DEB_DIR=deb
DEB=$(PACKAGE_DIR).deb
BIN=target/release/$(APPLICATION)

MACOS=macos
MACOS_APP=OTPTray.app

RUST_SOURCES := $(shell find src -name '*.rs')

UBUNTU_FOCAL_PKG=$(PACKAGE_DIR)_focal.deb
UBUNTU_HIRSUTE_PKG=$(PACKAGE_DIR)_hirsute.deb

$(UBUNTU_FOCAL_PKG):
	vagrant up focal64-build
	vagrant ssh focal64-build -c 'cd /vagrant && make deep_clean all'
	mv $(DEB) $@
focal: $(UBUNTU_FOCAL_PKG)

$(UBUNTU_HIRSUTE_PKG):
	vagrant up hirsute64-build
	vagrant ssh hirsute64-build -c 'cd /vagrant && make deep_clean all'
	mv $(DEB) $@
hirsute: $(UBUNTU_HIRSUTE_PKG)

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

$(MACOS_APP): $(BIN)
	mkdir -p $(MACOS_APP)/Contents/MacOS
	mkdir -p $(MACOS_APP)/Contents/Resources
	cp $(BIN) $(MACOS_APP)/Contents/MacOS
	cp $(MACOS)/Info.plist $(MACOS_APP)/Info.plist

clean:
	rm -rf $(PACKAGE_DIR) *.deb $(MACOS_APP)

deep_clean: clean
	cargo clean

.PHONY: clean deep_clean all hirsute focal
