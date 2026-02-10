{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };
  outputs =
    inputs@{
      self,
      nixpkgs,
      systems,
      devenv,
      treefmt-nix,
      ...
    }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);
      test-vm =
        {
          pkgs,
          lib,
          ...
        }:
        {
          system.stateVersion = "23.11";
          programs.sway.enable = true;
          services.greetd = {
            enable = true;
            settings = {
              default_session = {
                command = lib.mkForce "${
                  self.packages."x86_64-linux".default
                }/bin/ndlm --session ${pkgs.sway}/bin/sway --theme-file ${
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
      packages = eachSystem (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "ndlm";
            version = "1.1.0";

            src = ./.;

            nativeBuildInputs = with pkgs; [
              pkg-config
            ];
            buildInputs = with pkgs; [
              cairo
              pango.dev
            ];

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            meta = {
              description = "Not (so) dumb login manager";
              homepage = "https://github.com/ivandimitrov8080/ndlm";
              license = pkgs.lib.licenses.mit;
            };
          };
          interactive = self.checks.${system}.default.driverInteractive;
          inherit (self.nixosConfigurations.default.config.system.build) vm;
        }
      );
      devShells = eachSystem (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              {
                languages.rust = {
                  enable = true;
                };
                packages = with pkgs; [
                  pkg-config
                  cairo
                  pango.dev
                ];
                git-hooks.hooks = {
                  nixfmt.enable = true;
                  deadnix.enable = true;
                  statix.enable = true;
                  rustfmt.enable = true;
                };
              }
            ];
          };
        }
      );
      checks = eachSystem (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.testers.runNixOSTest {
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
          };
        }
      );
      formatter = eachSystem (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        (treefmt-nix.lib.evalModule pkgs {
          projectRootFile = "flake.nix";
          programs = {
            nixfmt.enable = true;
            deadnix.enable = true;
            statix.enable = true;
            rustfmt.enable = true;
          };
        }).config.build.wrapper
      );
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
    };
}
