{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };
  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      ndlm = (
        pkgs.rustPlatform.buildRustPackage rec {
          pname = "ndlm";
          version = "1.0";
          src = ./.;
          cargoHash = "sha256-iA8qkIrXJ3hf4V34MLGwt4yChkmjpo67oQvTJf5R+uw=";
          meta = {
            mainProgram = pname;
          };
        }
      );
      test-vm =
        { pkgs, lib, ... }:
        {
          system.stateVersion = "23.11";
          programs.sway.enable = true;
          services.greetd = {
            enable = true;
            settings = {
              default_session = {
                command = lib.mkForce "${ndlm}/bin/ndlm --session ${pkgs.sway}/bin/sway --theme-file ${
                  (pkgs.catppuccin-plymouth.override { variant = "mocha"; })
                }/share/plymouth/themes/catppuccin-mocha/catppuccin-mocha.plymouth";
                user = "greeter";
              };
            };
          };
          users.users = {
            test = {
              isNormalUser = true;
              password = "test";
            };
            greeter = {
              extraGroups = [
                "video"
                "input"
                "render"
              ];
            };
          };
          fileSystems."/" = {
            device = "/dev/vda";
            fsType = "ext4";
          };
          boot.loader.grub.devices = [ "/dev/vda" ];
        };
    in
    {
      nixosConfigurations.default = nixpkgs.lib.nixosSystem {
        modules = [
          test-vm
          {
            nixpkgs.hostPlatform = "x86_64-linux";
          }
        ];
      };
      packages.${system}.default = self.nixosConfigurations.default.config.system.build.vm;
      checks.${system}.default = pkgs.testers.runNixOSTest {
        name = "test";
        nodes = {
          machine = test-vm;
        };
        testScript =
          #py
          ''
            import time;
            start_all;
            machine.wait_for_unit("greetd.service");
            time.sleep(1)
            machine.send_key("t");
            machine.send_key("e");
            machine.send_key("s");
            machine.send_key("t");
            machine.send_key("ret");
            machine.send_key("t");
            machine.send_key("e");
            machine.send_key("s");
            machine.send_key("t");
            machine.send_key("ret");
            machine.succeed("loginctl list-sessions | grep test");
            machine.succeed("journalctl -u greetd | grep 'Session started'");
          '';
      };
    };
}
