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
          cargoHash = "sha256-ZP+kDc5Q1wA6du0gvniMoG8DezmYW8trxSpvxVdzccg=";
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
      testNdlmCanLogin = (
        pkgs.testers.runNixOSTest {
          name = "test";
          nodes = {
            machine = test-vm;
          };
          testScript =
            #py
            ''
              machine.wait_for_unit("multi-user.target");
              machine.send_chars("test\ntest\n");
              machine.sleep(1)
              machine.succeed("loginctl list-sessions | grep test");
            '';
        }
      );
    in
    {
      nixosConfigurations = {
        default = nixpkgs.lib.nixosSystem {
          modules = [
            test-vm
            {
              nixpkgs.hostPlatform = "x86_64-linux";
            }
          ];
        };
      };
      packages.${system} = {
        default = self.nixosConfigurations.default.config.system.build.vm;
        interactive = testNdlmCanLogin.driverInteractive;
      };
      checks.${system} = {
        default = testNdlmCanLogin;
      };
    };
}
