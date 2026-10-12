<p align="center"><img src="assets/altr.png" width="96" alt="ALTR"></p>

<h1 align="center">ALTR</h1>

<p align="center">Cambia entre tus cuentas de Valorant y League of Legends con un clic.</p>

## Instalar

1. Descarga `ALTR-win-Setup.exe` de la [última versión](https://github.com/davebsrk/altr/releases/latest).
2. Ábrelo. Se instala solo para tu usuario (no pide permisos de administrador) y crea un acceso directo.
3. La primera vez Windows puede avisar con "Windows protegió tu PC": pulsa **Más información → Ejecutar de todas formas** (el programa no está firmado).

ALTR se actualiza solo: cuando haya una versión nueva verás "ALTR x.y.z disponible · Actualizar".

## Cómo se usa

- **+** añade una cuenta: se abre la ventana de Riot, inicias sesión una vez marcando **«Mantener sesión iniciada»** y ALTR la guarda.
- **Clic en una cuenta** para cambiar a ella (con confirmación). **Clic derecho** para eliminarla.
- **JUGAR** abre Valorant o LoL con la cuenta activa; si el juego necesita actualizarse, ALTR lo actualiza primero.
- El rango y el nivel se guardan mientras juegas y se muestran aunque el juego esté cerrado.

El Riot Client trabaja oculto en segundo plano; su ventana solo aparece para iniciar sesión.

## Seguridad

- ALTR nunca ve ni guarda contraseñas: guarda la sesión que el propio Riot Client crea al iniciar sesión, cifrada con DPAPI de Windows (solo tu usuario en tu PC puede descifrarla).
- No toca los juegos ni Vanguard, no lee memoria y no modifica archivos de los juegos. Solo habla con la API local del Riot Client (127.0.0.1), igual que su propia ventana.
- No cambia de cuenta con un juego abierto ni con el cliente de LoL abierto (cerrarlo en selección de campeón contaría como abandono).
- Proyecto no oficial, sin relación con Riot Games.

## Desarrollo

Requisitos: Windows, [Rust](https://rustup.rs) y las Build Tools de Visual Studio (C++).

```
cargo run -p altr            # ejecutar
cargo test --workspace       # tests
cargo build --release -p altr
```

Herramientas de depuración (solo en builds de desarrollo): `altr --probe`, `altr --do …` y `altr --snapshot …` (ver `DESIGN.md`).

### Publicar una versión

1. Sube la versión en `Cargo.toml` (`[workspace.package] version`).
2. Crea y sube la etiqueta: `git tag v0.2.0 && git push origin v0.2.0`.
3. GitHub Actions compila, empaqueta con Velopack y publica la release; las apps instaladas la ofrecerán al abrirse.

Diseño y decisiones técnicas: [DESIGN.md](DESIGN.md). Fuente Barlow bajo licencia SIL OFL 1.1 (`app/ui/fonts/OFL.txt`).
