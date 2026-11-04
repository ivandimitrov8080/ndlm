{
  inputs = {
    configuration.url = "github:ivandimitrov8080/configuration.nix";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    catppuccin.url = "github:catppuccin/nix";
    catppuccin.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs =
    inputs@{
      self,
      configuration,
      nixpkgs,
      systems,
      devenv,
      treefmt-nix,
      catppuccin,
      ...
    }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);
      test-vm =
        {
          ...
        }:
        {
          imports = [
            configuration.nixosModules.default
            catppuccin.nixosModules.catppuccin
          ];
          nixpkgs.overlays = [
            configuration.overlays.default
            (final: prev: {
              ndlm = self.packages.x86_64-linux.default;
            })
          ];
          catppuccin = {
            enable = true;
            autoEnable = true;
            accent = "green";
            flavor = "mocha";
          };
          system.stateVersion = "26.05";
          programs.sway.enable = true;
          boot = {
            loader.limine = {
              resolution = "1920x1080";
              style.interface.resolution = "1920x1080";
            };
            initrd = {
              kernelModules = [ "i915" ];
            };
          };
          # programs.hyprland.enable = true;
          meta.graphicalBoot.enable = true;
          meta.graphicalBoot.plymouthTheme = "/etc/plymouth/themes/catppuccin-mocha/catppuccin-mocha.plymouth";
          users.users = {
            test = {
              isNormalUser = true;
              password = "test";
              extraGroups = [ "wheel" ];
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
