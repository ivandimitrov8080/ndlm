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
          system.stateVersion = "25.11";
          programs.sway.enable = true;
          services.greetd = {
            enable = true;
            settings = {
              default_session = {
                command = lib.mkForce "${self.packages."x86_64-linux".default}/bin/ndlm --theme-file ${
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
          default = pkgs.rustPlatform.buildRustPackage rec {
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
              description = "Not (so) dummy login manager";
              homepage = "https://github.com/ivandimitrov8080/ndlm";
              license = pkgs.lib.licenses.mit;
              mainProgram = pname;
            };
          };
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
                devenv.root = "/home/ivand/src/ndlm";
                languages.rust = {
                  enable = true;
                };
                packages = with pkgs; [
                  pkg-config
                  cairo
                  pango.dev
                  cargo-edit
                ];
                git-hooks.hooks = {
                  nixfmt.enable = true;
                  deadnix.enable = true;
                  statix.enable = true;
                  rustfmt.enable = true;
                  rustfmt.settings.config = {
                    edition = "2024";
                  };
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
          integrationTest = pkgs.testers.runNixOSTest {
            name = "test";
            nodes = {
              machine = test-vm;
            };
            testScript =
              #py
              ''
                machine.wait_for_unit("multi-user.target");
                machine.send_chars("test\ntest\n");
                machine.sleep(2)
                machine.succeed("loginctl list-sessions | grep test");
              '';
          };
        }
        // self.packages.${system}
        // self.devShells.${system}
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
