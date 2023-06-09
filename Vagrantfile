# -*- mode: ruby -*-
# vi: set ft=ruby :

# Vagrantfile API/syntax version. Don't touch unless you know what you're doing!
VAGRANTFILE_API_VERSION = "2"

$ubuntu_build = <<-SCRIPT
apt-get update
apt-get install -y libappindicator3-dev libgtk-3-dev make build-essential libclang-dev
wget https://sh.rustup.rs -O rustup.sh
chmod +x rustup.sh
su vagrant -c './rustup.sh -y'
echo 'export PATH=$PATH:$HOME/.cargo/bin' >> /home/vagrant/.bashrc
SCRIPT

Vagrant.configure(VAGRANTFILE_API_VERSION) do |config|
  config.vm.define "focal64-build" do |c|
    c.vm.hostname = "focal64-build"
    c.vm.box = "ubuntu/focal64"
    c.vm.network "private_network", ip: "192.168.55.12"
    c.vm.provider "virtualbox" do |vb|
      vb.gui = false
      vb.memory = "2048"
    end
    c.vm.provision "shell", inline: $ubuntu_build
  end

  config.vm.define "focal64" do |c|
    c.vm.hostname = "focal64"
    c.vm.box = "ubuntu/focal64"
    c.vm.network "private_network", ip: "192.168.55.13"
    c.vm.provider "virtualbox" do |vb|
      vb.gui = true
      vb.memory = "2048"
    end
  end

  config.vm.define "hirsute64-build" do |c|
    c.vm.hostname = "hirsute64-build"
    c.vm.box = "ubuntu/hirsute64"
    c.vm.network "private_network", ip: "192.168.55.10"
    c.vm.provider "virtualbox" do |vb|
      vb.gui = false
      vb.memory = "2048"
    end
    c.vm.provision "shell", inline: $ubuntu_build
  end

  config.vm.define "hirsute64" do |c|
    c.vm.hostname = "hirsute64"
    c.vm.box = "ubuntu/hirsute64"
    c.vm.network "private_network", ip: "192.168.55.11"
    c.vm.provider "virtualbox" do |vb|
      vb.gui = true
    end
  end
end
