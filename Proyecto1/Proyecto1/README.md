# Captura la Bandera

Implementacion base en Rust del PRFC-CC8-2026 con:

- servidor autoritativo por TCP;
- descubrimiento UDP broadcast;
- protocolo binario big-endian version `3`;
- cliente y servidor con interfaz grafica en macroquad.

## Ejecutar

Cuando Rust este instalado:

```powershell
cargo run -- server
```

En otra terminal:

```powershell
cargo run -- client
```

En Windows, `cargo build`/`cargo run` necesitan el linker de MSVC (Visual
Studio Build Tools, componente "Desarrollo para el escritorio con C++"),
ademas del toolchain de Rust instalado con `rustup`.

## Estado

La base cubre:

- lobby y aceptacion/rechazo de jugadores;
- countdown de inicio;
- loop del juego en el servidor;
- movimiento en cuatro direcciones;
- interaccion para tomar o robar la bandera;
- victoria al salir completamente del circulo con la bandera;
- desconexion con caida de bandera;
- vista del servidor y vista del jugador en macroquad.
