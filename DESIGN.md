# ALTR — Documento de diseño

Launcher multicuenta para Riot (Valorant y League of Legends) en Windows. Antes se llamó ValoLauncher; los datos de `%APPDATA%\ValoLauncher` se trasladan solos a `%APPDATA%\ALTR` (sin las sesiones, que estaban cifradas con el nombre antiguo y hay que volver a guardar con +).
Objetivo: cambiar de cuenta en ~3 s sin cerrar sesión a mano, con una app ligera, simple y moderna.

## Decisiones cerradas

| Tema | Decisión |
|---|---|
| Lenguaje / UI | Rust + Slint |
| Juegos | Valorant y League of Legends |
| Layout | Variante C: barra lateral de cuentas + panel de detalle |
| Tema | Claro / oscuro / sistema (por defecto: sistema) |
| Acentos | 6 colores seleccionables |
| Cambio de cuenta | Clic en la barra lateral + confirmación; guardar y restaurar la sesión del Riot Client (cifrada con DPAPI) |
| Rango y nivel | Caché local, leída del cliente cuando la cuenta está activa |

## Interfaz

```
┌──────────────────────────────────────────────────────────┐
│ ■ ALTR                                   —   ✕   │  barra de título nativa de Windows
├──────┬───────────────────────────────────────────────────┤
│ [MN] │ Main#EUW                          [V] [L]      │  iconos oficiales de los juegos
│ [SM] │ ● Conectada · LAN                                 │
│ [LL] │                                                   │
│ [NA] │ ┌───────────────────────────────────────────────┐ │
│      │ │ 🏆 Rango competitivo              Nivel        │ │
│      │ │    Ascendente 2 · 67 RR           214          │ │
│      │ └───────────────────────────────────────────────┘ │
│  +   │                                                   │
│  ⚙   │ Actualizando LoL · 62 % ━━━━━  [ ▶ Jugar ]        │  barra inferior + botón compacto
└──────┴───────────────────────────────────────────────────┘
```

- **Barra lateral (68 px):** avatar con iniciales por cuenta, anillo de acento en la seleccionada, insignia `V`, `L` o `V·L` según los juegos de la cuenta. Abajo: añadir cuenta y ajustes. **Reordenable arrastrando** (botón izquierdo, a partir de 6 px de movimiento; no cuenta como clic): el avatar sigue al ratón, los demás se apartan y un recuadro de acento marca dónde caerá. No se puede arrastrar mientras hay una tarea en curso.
- **Panel de detalle:** Riot ID (o el **alias** si la cuenta tiene uno; entonces el Riot ID pasa a la línea de estado: "● Conectada · EU · Main#EUW"); debajo "● Conectada · LAN" (cuenta activa) o "Usada hace 2 días · LAN"; tarjeta de rango y nivel.
- **Selector de juego:** dos iconos de 44 px con el logo oficial de cada juego, leído de la caché del propio Riot Client (`C:\ProgramData\Riot Games\Metadata\<producto>.live\<producto>.live.ico`, 256 px, escalado una vez a 64 px; si falta, una letra). El seleccionado va con borde de acento y opacidad completa; el otro, atenuado (se ilumina al pasar el ratón). Un punto de acento marca una actualización pendiente; mientras un juego **no seleccionado** se actualiza, un anillo de progreso rodea su icono.
- **Barra inferior:** a la izquierda una **tarjeta de estado** (borde de color, icono o spinner, título, detalle en segunda línea, botón pequeño y línea de progreso en el borde inferior): "Esperando inicio de sesión · Marca «Mantener sesión iniciada»…" con "Cancelar"; "Actualizando League of Legends · 62 % · 3,2 de 5,1 GB · 12 MB/s"; "Actualización de LoL · Pulsa ACTUALIZAR"; o en verde "Nueva versión 0.1.2 · Se instala en segundos" con "Actualizar". A la derecha, botón compacto de 190 px ("JUGAR", "ACTUALIZAR", o "JUGAR AL TERMINAR" → "SE ABRIRÁ AL TERMINAR" durante la actualización del juego elegido). Al abrir un juego, el propio botón muestra un spinner y "ABRIENDO…" hasta que aparece el proceso del juego (sin aviso al terminar; si no aparece en 90 s, error). Si el juego elegido ya está abierto (se comprueba cada 2 s), el botón queda desactivado como "ABIERTO". Se oculta mientras hay otra tarea y la tarjeta ocupa la fila.
- **Animaciones (150–220 ms):** fundido de la tarjeta y el botón al cambiar de juego o de cuenta; Ajustes y paneles con fundido + desplazamiento de 12 px; el anillo de selección del avatar se desliza a la cuenta elegida; diálogos y menú contextual con fundido de entrada; aviso flotante con fundido y deslizamiento.
- **Iniciales:** si dos cuentas comparten iniciales (mismo nombre), el avatar añade el inicio del tag: "D19", "D90".
- **Resultados:** al terminar una acción, un aviso flotante con icono (✓ verde o ! rojo) aparece sobre el botón y se oculta solo (4 s, errores 7 s; clic para cerrarlo).
- **Cambiar de cuenta:** clic en otra cuenta de la barra lateral → diálogo "¿Cambiar a Nombre#TAG?" (Enter confirma, Escape o clic fuera cancela) → se reinicia el Riot Client con esa cuenta. Clic en la cuenta ya activa: no hace nada. La confirmación se puede desactivar en Ajustes ("Pedir confirmación antes de cambiar", activada por defecto).
- **Menú contextual de cuenta (clic derecho):** popup propio con el diseño de la app (no el menú nativo de Windows):
  - "Cambiar nombre": diálogo con el nombre seleccionado (Enter guarda, Escape cancela). El alias solo se ve en ALTR (avatar, cabecera, diálogos y bandeja); vacío o igual al nombre de Riot lo borra; máximo 24 caracteres.
  - Casillas "Valorant" y "League of Legends" con el logo de cada juego: a qué juega la cuenta (insignia del avatar y juego que se muestra al elegirla). La última casilla marcada no se puede quitar.
  - "Eliminar cuenta" en rojo, con confirmación (botón rojo sea cual sea el acento).
  - Los diálogos abiertos desde el menú toman el foco cuando el menú termina de cerrarse (si no, Slint se lo devolvía a la ventana y Enter pulsaba "Jugar").
- **Atajos de teclado:** Ctrl+1…9 cambia a esa cuenta (con la misma confirmación que el clic); Intro hace lo del botón principal (Jugar, Actualizar o Jugar al terminar); Escape cierra Ajustes y los diálogos. Se recuerdan en una línea al pie de Ajustes.
- **Bandeja del sistema:** icono de ALTR con menú nativo: las cuentas (✓ la activa; elegir una muestra la ventana con la confirmación), "Jugar Valorant", "Jugar League of Legends", "Abrir ALTR" y "Salir". Clic izquierdo en el icono: abre la ventana. Cerrar la ventana la oculta en la bandeja ("Al cerrar la ventana, seguir en la bandeja del sistema", activado por defecto; desactivado, cerrar sale y no hay icono). "Salir" con una tarea en curso no sale: muestra la ventana y avisa (cortar un cambio de cuenta a medias dejaría mezclados los archivos de sesión). Si algo lanzado desde la bandeja falla, la ventana se muestra para enseñar el error.
- **Abrir con Windows** (Ajustes, desactivado por defecto): valor `ALTR` en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` con `"<ruta>\altr.exe" --background` (sin admin; aparece en "Aplicaciones de arranque" del Administrador de tareas). Con `--background` ALTR arranca solo en la bandeja; si la bandeja está desactivada, abre la ventana (si no, no habría forma de volver a ella). La ruta se reescribe en cada arranque y el desinstalador de Velopack borra el valor. Las builds de desarrollo usan `ALTR (dev)`.
- **Ajustes** se desplaza con la rueda cuando la ventana es más baja de lo que necesita (tamaño mínimo).
- **Ventana:** 720×440 (mínimo 600×400), barra de título nativa de Windows 11: sigue el tema elegido y conserva snap layouts y redimensionado.

### Temas

| Token | Oscuro | Claro |
|---|---|---|
| `bg` | `#0F1115` | `#F6F7F9` |
| `sidebar` | `#0B0D10` | `#ECEEF2` |
| `card` | `#171A20` | `#FFFFFF` |
| `border` | `#262A33` | `#E1E4EA` |
| `text` | `#E8EAED` | `#15171C` |
| `muted` | `#8A909C` | `#6B7280` |

### Tipografía

Barlow (SIL OFL 1.1, de estilo DIN como la interfaz de Valorant), incluida en el ejecutable (`app/ui/fonts/`, con su `OFL.txt`): Regular y SemiBold para toda la app; Barlow Semi Condensed SemiBold en mayúsculas con 0,8 px de espaciado solo para el botón principal ("JUGAR"). Release: 9,9 MB.

### Acentos

| Nombre | Color |
|---|---|
| Rojo (por defecto) | `#FF4655` |
| Oro | `#C8AA6E` |
| Azul | `#3B82F6` |
| Violeta | `#8B5CF6` |
| Turquesa | `#14B8A6` |
| Rosa | `#EC4899` |

En Ajustes cada color muestra su nombre debajo.

El tinte suave del acento (fondo del icono de rango, por ejemplo) es el mismo color al 15 % de opacidad.

## Funcionamiento

### Rutas de Riot (verificadas en esta máquina)

- Instalaciones: `C:\ProgramData\Riot Games\RiotClientInstalls.json` → `rc_default` apunta a `RiotClientServices.exe`.
- Sesión: `%LOCALAPPDATA%\Riot Games\Riot Client\Data\RiotGamesPrivateSettings.yaml`.
- `Data\Sessions\` solo contiene un heartbeat; no hace falta guardarlo.
- API local del Riot Client: `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`.
- API local de LoL (LCU): `lockfile` en la carpeta de instalación de League of Legends.

### Riot Client oculto

El launcher mantiene el Riot Client sin ventana (`RiotClientServices.exe --headless`): conserva la sesión, responde a la API local y lanza los juegos, sin la interfaz de Riot (~300–500 MB menos de RAM). Su ventana solo aparece para iniciar sesión.

- **Nunca cerrar solo la ventana** (`Riot Client.exe`): el servicio lo interpreta como salir y **cierra la sesión** (comprobado). Para ocultarla se reinicia el cliente en modo oculto, que vuelve a entrar con la sesión guardada.
- Al cerrar el cliente se termina primero `RiotClientServices.exe` y después la ventana.
- Un cliente oculto ignora un relanzamiento normal; para mostrar la ventana se cierra y se abre sin `--headless`.
- Iniciar sesión dentro del launcher se descartó: Riot exige hCaptcha (necesita un navegador) y la contraseña pasaría por la app.

### Cambiar de cuenta

1. Si Valorant (`VALORANT.exe`, `VALORANT-Win64-Shipping.exe`), una partida de LoL (`League of Legends.exe`) o el cliente de LoL (`LeagueClientUx.exe`, porque cerrarlo en selección de campeón cuenta como abandono) están abiertos, o un juego se está actualizando (reiniciar el cliente cortaría la descarga) → no hacer nada y avisar.
2. Preguntar al cliente (API local) de quién es la sesión actual y, tras cerrarlo, guardarla en esa cuenta para que sus tokens no caduquen.
3. Cerrar el cliente (servicio primero).
4. Descifrar la sesión guardada de la cuenta y escribirla en `RiotGamesPrivateSettings.yaml`.
5. Lanzar el cliente oculto ("Conectando con Nombre#TAG…"). ~7 s en total.
6. Cuando la API local confirma (hasta 45 s) la sesión, volver a guardar el archivo para que no caduque.
7. Si la sesión ha caducado → se abre la ventana de Riot en su pantalla de login y el launcher avisa: iniciar sesión (con «Mantener sesión iniciada») y pulsar + para guardar la sesión nueva.

### Jugar

`POST /product-launcher/v1/products/<valorant|league_of_legends>/patchlines/live` en la API local, lo mismo que el botón Jugar de Riot (si el cliente no está abierto, se arranca oculto primero). Los argumentos del acceso directo (`--launch-product=…`) no se usan: con el cliente abierto solo lo enfocan.

Si el juego necesita actualizarse o repararse, el cliente responde `424 launch_failed_product_not_up_to_date` **y empieza él mismo a actualizarlo** (`PutPriorityPatchRequestV2`, dependencias incluidas). El launcher sigue ese progreso dentro del botón y lanza el juego al terminar.

### Actualizaciones

- **Estado:** `GET /patch-proxy/v2/patch-states/products/<producto>/patchlines/live`, el mismo que usa la ventana de Riot → `state` (`UpToDate`; `OutOfDate`/`NeedsRepair`/`Paused` = pendiente; `Updating`/`Repairing`/`DependencyUpdate` = en curso; `StatusError`) y `progress.overallProgress` (0–100), `totalBytesDownloaded`/`totalBytesToDownload`. **No usar** `/patch/v1/installs/<producto>.live/status`: compara con el manifiesto que ya tiene y dice `up_to_date` ante una versión nueva hasta que empieza el parche (comprobado: Riot ofrecía 4,7 GB y ALTR solo mostraba "Jugar"). La parte `game_patch` de LoL la gestiona el propio cliente de LoL y Riot la ignora para "al día". Estados desconocidos (`CheckingForUpdate` con el cliente recién arrancado) no marcan nada. **Solo responde con sesión iniciada:** el cliente que Riot arranca con Windows (`--launch-background-mode`) y el oculto tras cerrar el juego no tienen sesión y contestan 404 a todo (así se coló un parche de Valorant con ALTR en "Jugar").
- **Aviso:** un punto de acento en el selector de juego y el botón pasa a "Actualizar <juego>" con icono de descarga. Se comprueba al abrir, tras cada acción y cada 10 minutos. Al abrir la ventana (como mucho cada 10 minutos), si el cliente no tiene sesión, ALTR lo inicia oculto con la sesión guardada (como Jugar) y espera a que Riot compruebe la versión (~12 s); no lo hace con un juego abierto, con la ventana de Riot visible ni sin cuentas. Las acciones esperan a que termine para no reiniciar el cliente a la vez.
- **Actualizar:** el mismo `POST` de Jugar (→ 424 → el cliente parchea con su flujo oficial). El botón se llena como barra de progreso ("Actualizando League of Legends… 34 %") con "Cancelar" al lado (`DELETE /patch/v1/installs/<id>/requests/patch`). Solo se da por terminada cuando "al día" se mantiene 5 s, porque el cliente puede encadenar una segunda pasada.
- **Descartado:** pedir el parche a bajo nivel (`PUT /patch/v1/installs/{id}/requests/patch` con la URL del manifiesto). Descarga los archivos pero el registro de productos de Riot no lo da por instalado y al reiniciar vuelve a descargarlo todo.

### Añadir una cuenta

1. Si el Riot Client ya tiene abierta una cuenta que no está guardada → guardarla directamente.
2. Si no: cerrar el Riot Client (guardando antes la sesión de la cuenta activa), borrar el archivo de sesión y abrir la ventana de Riot en su login. El botón muestra "Inicia sesión en la ventana de Riot y marca «Mantener sesión iniciada»" con "Cancelar" (espera hasta 5 min).
3. Detectar el login vía la API local (`/rso-auth/v1/authorization`) y leer PUUID (`sub`), Riot ID (`acct`) y región de la cuenta (`region.tag`, p. ej. "lan") de `/rso-auth/v1/authorization/userinfo`. (La región del chat no sirve: una cuenta LAN aparece como "br1".)
4. Guardar la sesión cifrada (si la cuenta ya existía, solo se actualiza), reiniciar el cliente oculto y traer el launcher al frente. El launcher nunca ve ni guarda contraseñas.
5. Si se cancela o se agota la espera → volver a conectar, oculta, la cuenta que estaba activa.

Al arrancar, si el Riot Client tiene abierta una cuenta guardada, el launcher actualiza su Riot ID y región, y la marca como activa si se cambió a mano.

### Caché de rango y nivel

Todo local, sin servidores ni claves: se lee del juego abierto al arrancar el launcher y cada minuto, se guarda en la cuenta que está jugando (por PUUID) con su fecha, y la tarjeta muestra "Clasificatoria solo/dúo · hace 3 min". Mientras hay una acción en curso no se toca el almacén.

- **LoL (LCU, la API local del cliente de LoL, permitida por Riot):** `lockfile` en la carpeta de LoL (de `RiotClientInstalls.json`). Nivel de `/lol-summoner/v1/current-summoner` (`summonerLevel`); solo/dúo de `/lol-ranked/v1/current-ranked-stats` → `queueMap.RANKED_SOLO_5x5` (`tier`, `division`, `leaguePoints`) → "Oro IV · 42 LP". Maestro, Gran Maestro y Retador sin división; sin rango → "Sin clasificar".
- **Valorant:** la presencia propia en el chat del Riot Client mientras el juego está abierto: `/chat/v4/presences`, entrada con el PUUID propio (de `/chat/v1/session`) y `product: valorant`; su `private` es JSON en base64 con `playerPresenceData.competitiveTier` (3–26 = Hierro 1 … Inmortal 3, 27 = Radiante) y `accountLevel`. **El RR no se publica en local**: se muestra solo el rango ("Ascendente 1"). Obtenerlo exigiría los servidores de Riot con los tokens de la cuenta; descartado.
- **API oficial de Riot (developer.riotgames.com):** descartada por ahora: necesita clave (la de desarrollo caduca cada 24 h; la clave no puede ir dentro de una app de escritorio) y el rango de Valorant no está disponible. Podría añadirse como extra para refrescar LoL sin abrir el juego.
- **Historial de rango:** descartado. Solo en local se quedaría corto (solo ve los momentos en que el juego está abierto con ALTR en marcha); hacerlo completo exigiría la API oficial con un servidor propio y la aprobación de Riot. Para eso ya están op.gg o tracker.gg.

**Cliente oculto y fin de partida:** cuando se cierra el juego que lanzó, un Riot Client en `--headless` cierra su sesión ("XMPP Logout… FINALIZE"), aunque el archivo de sesión sigue siendo válido. Antes de jugar o actualizar, el launcher comprueba la sesión y, si no la hay y no hay ningún juego abierto, reinicia el cliente oculto, que vuelve a entrar solo (~13 s en lugar de ~9 s). Mientras tanto la comprobación de actualizaciones no ve nada hasta la siguiente acción. Lanzar un juego tarda ~10 s en responder: las peticiones de acción esperan hasta 60 s (las de lectura, 3 s). Si tras actualizar el cliente sigue verificando/reparando, "Jugar" reintenta cada 5 s hasta 2 min.

### Seguridad y riesgo

- Sesiones cifradas con DPAPI (`CryptProtectData`, ámbito usuario): solo se pueden descifrar con tu usuario en tu PC.
- El launcher no toca nunca el proceso del juego ni Vanguard, no lee memoria y no modifica archivos del juego.
- Sin telemetría ni conexiones propias a internet.

## Arquitectura

```
altr/
├─ Cargo.toml          workspace
├─ core/               crate `altr-core`: sin UI, testeable
│  ├─ riot/            rutas, procesos, lanzamiento, API local, LCU
│  ├─ vault/           sesiones cifradas con DPAPI
│  ├─ accounts/        modelo de cuenta + caché de rango
│  └─ config/          tema, acento, juego por defecto (TOML)
└─ app/                ejecutable `altr`
   ├─ ui/              app.slint, components.slint, theme.slint, icons/
   └─ src/             main.rs (une core y UI) + snapshot.rs (solo debug)
```

**Datos del usuario:**
- `%APPDATA%\ALTR\config.toml`: ajustes.
- `%APPDATA%\ALTR\accounts.toml`: alias, juegos y caché de rango (sin secretos).
- `%APPDATA%\ALTR\vault\<id>.bin`: sesión cifrada.

**Renderizado:** renderizador por software de Slint (sin OpenGL ni GPU), suficiente para una ventana de este tamaño y mucho más ligero en RAM.

**Captura sin ventana (solo debug):** `altr --snapshot out.bmp [--dark|--light] [--settings] [--account N] [--accent N]` dibuja la UI en una imagen sin abrir ninguna ventana.

**Dependencias:** `slint`, `windows-sys` (DPAPI y procesos), `serde` + `toml` + `serde_json`, `ureq` con TLS nativo de Windows (Schannel) para la API local. Release con `opt-level = "s"`: 9,3 MB.

**Diagnóstico (solo debug, solo lectura):** `altr --probe` muestra lo que el launcher ve de Riot (ruta del cliente, sesión, procesos, estado de la API) sin tocar nada.

**Pruebas en real (solo debug):** `altr --do add | add-cancel S | switch N | remove N | play|update valorant|league` ejecuta las mismas funciones que la UI e imprime el resultado (`add-cancel S` simula pulsar Cancelar a los S segundos).

## Objetivos de rendimiento

| Métrica | Objetivo |
|---|---|
| Ejecutable | < 10 MB |
| RAM en reposo | < 30 MB |
| Arranque | < 300 ms |
| Cambio de cuenta | ~3 s |

## Fases

1. **Esqueleto:** workspace, UI de la variante C con datos falsos, temas y acentos. (Hecho.)
2. **Núcleo:** detección de Riot, guardar y restaurar sesión, añadir y eliminar cuenta, cambio real. (Hecho y probado en real el 2026-10-12: añadir, cambio con restauración de sesión, login nuevo hasta la pantalla de login, eliminar, Jugar Valorant y bloqueo con el juego abierto. Cambio entre dos cuentas distintas probado también.)
3. **Caché de rango:** LCU para LoL y presencia del chat para Valorant. (Hecho 2026-10-12; LoL probado en real: "Oro IV · 42 LP", nivel 109. Valorant también visto en real: el rango aparece, sin RR.)
4. **Pulido:** arrastrar para reordenar, atajos de teclado, icono en la bandeja, alias y juegos por cuenta en el clic derecho, instalador. (Hecho 2026-10-12. Arrastre, atajos y menú comprobados con `--snapshot x.bmp --uitest`, que envía eventos reales de ratón y teclado a la interfaz sin ventana; bandeja comprobada en real: cerrar oculta, un segundo lanzamiento la muestra, sin bandeja cerrar sale.)

Hecho fuera de fase (2026-10-12): Riot Client oculto, actualizaciones desde el launcher, "Jugar" que actualiza solo y abrir con Windows (v0.1.5).

**Estado (2026-10-15):** todo lo anterior publicado en la v0.1.5 y confirmado funcionando por el usuario. La v0.1.6 corrige la detección de actualizaciones de los juegos (patch-proxy; probada en vivo actualizando Valorant). La v0.1.7 la hace funcionar con el cliente sin sesión (lo inicia oculto al abrir la ventana; probado en vivo, falta verlo con un parche pendiente). Ideas sin empezar: notas por cuenta en el clic derecho.

## Marca

- **Nombre:** ALTR (de "alt", cuenta secundaria).
- **Logo:** dos tarjetas de cuenta, la activa delante en rojo `#FF4655` sobre fondo `#0F1115`. Fuente: `assets/altr.svg`. `cargo run -p altr --example make_icon` genera `assets/altr.ico` (16–256 px) y `assets/altr.png` (256 px).
- El `.ico` va dentro del ejecutable (`winresource` en `app/build.rs`, con ProductName/FileDescription) y la ventana usa el SVG.

## Distribución y actualizaciones

- **Ejecutable:** `altr.exe` sin dependencias (CRT estático en `.cargo/config.toml`; no necesita el redistribuible de VC++). Release con `opt-level = "z"`: ~11 MB.
- **Instalador y actualizaciones: Velopack.** `Setup.exe` instala por usuario en `%LocalAppData%\ALTR` (sin admin), con acceso directo y desinstalador.
- **Publicación:** al subir una etiqueta `vX.Y.Z`, `.github/workflows/release.yml` compila en Windows, empaqueta con `vpk` (misma versión que el crate `velopack`) y publica la release en GitHub con el `Setup.exe` y los paquetes (completo y delta). La versión de la etiqueta debe coincidir con la de `Cargo.toml` (el workflow lo comprueba).
- **En la app:** `VelopackApp::build().run()` es lo primero de `main`. Al abrir, si se instaló con el Setup, consulta las releases de GitHub; si hay una versión nueva, abajo a la izquierda aparece "ALTR 0.2.0 disponible · Actualizar": descarga (solo el delta si puede), cierra, aplica y vuelve a abrir. En builds de desarrollo no hace nada.
- **SmartScreen:** el ejecutable no está firmado; la primera vez Windows avisa ("Más información → Ejecutar de todas formas").
- **Una sola copia abierta:** un mutex con nombre (`Local\ALTR`) impide abrir ALTR dos veces (dos copias guardarían cada una su versión de las cuentas y se pisarían los cambios); la segunda avisa a la primera con un evento con nombre (`Local\ALTR-show`, que la primera consulta cada 500 ms) para que muestre su ventana aunque esté oculta en la bandeja, y se cierra. Las builds de desarrollo usan `Local\ALTR-dev` para poder abrirse junto al ALTR instalado. Las herramientas de depuración no están afectadas.
