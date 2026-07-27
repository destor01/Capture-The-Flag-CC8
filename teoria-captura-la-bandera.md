# Teoría completa: Captura la Bandera (Rust + macroquad + Sockets)

## Índice
1. Arquitectura general del sistema
2. Rust: conceptos clave usados en el proyecto
3. Redes: TCP vs UDP, por qué se usa cada uno
4. El protocolo PRFC-CC8-2026
5. Modelo cliente-servidor autoritativo
6. El game loop y los ticks
7. macroquad: gráficos y game loop
8. Concurrencia: hilos, mutex, y por qué importa el orden de bloqueo
9. RadminVPN y por qué el discovery es un caso especial
10. Preguntas típicas que te pueden hacer (y cómo responderlas)

---

## 1. Arquitectura general del sistema

El proyecto tiene tres piezas grandes que interactúan:

```
┌─────────────┐         TCP:5000          ┌─────────────┐
│   CLIENTE   │ ◄──────────────────────► │   SERVIDOR   │
│ (macroquad) │   Input/Interact/Leave     │ (autoritativo)│
│             │ ◄── GameState/Events ────  │              │
└─────────────┘                            └─────────────┘
      ▲                                           ▲
      │         UDP:5001 (broadcast)              │
      └──────── DISCOVER_REQUEST/RESPONSE ─────────┘
```

- **El servidor** es el único que decide qué es verdad en el juego: posiciones, quién tiene la bandera, quién ganó. Es la fuente única de verdad ("autoritativo").
- **El cliente** solo hace dos cosas: (1) manda lo que el jugador quiere hacer (moverse, interactuar), y (2) dibuja en pantalla lo que el servidor le dice que está pasando. El cliente **nunca decide** el resultado del juego por sí mismo.
- **El discovery UDP** es un mecanismo aparte, opcional, solo para que el cliente encuentre servidores automáticamente sin que alguien le pase la IP a mano.

Esta separación (servidor autoritativo + clientes "tontos") es el patrón estándar en juegos multijugador en red, porque evita que un jugador con el cliente modificado pueda hacer trampa (por ejemplo, decir "yo agarré la bandera" sin que sea cierto) — el cliente pide, el servidor decide.

---

## 2. Rust: conceptos clave usados en el proyecto

No hace falta saber todo Rust, pero sí estos conceptos porque aparecen todo el tiempo en el código:

### Ownership y borrowing
Cada valor en Rust tiene un único "dueño". Cuando pasás un valor a una función, o lo movés a otra variable, quien lo tenía antes deja de poder usarlo (a menos que uses referencias `&` o clones `.clone()`). Esto es lo que hace que Rust no tenga garbage collector y sea seguro en memoria sin overhead en tiempo de ejecución.

### `Option<T>` y `Result<T, E>`
En vez de valores nulos (`null`) o excepciones, Rust usa:
- `Option<T>`: algo puede ser `Some(valor)` o `None` (no hay valor). Por ejemplo, `player_id: Option<u32>` — puede que un jugador todavía no tenga ID asignado.
- `Result<T, E>`: una operación puede salir bien (`Ok(valor)`) o mal (`Err(error)`). Se usa mucho en lectura/escritura de sockets, donde la conexión se puede cortar en cualquier momento.

Esto es directamente relevante al bug de "jugadores fantasma" que se corrigió: `ConnectionClosed { player_id: None }` — el `None` significa "este jugador se desconectó antes de que le asignáramos un ID", y el código original no manejaba ese caso.

### Structs y enums
- Los `struct` agrupan datos (por ejemplo, `Player { x: f32, y: f32, has_flag: bool }`).
- Los `enum` representan "una de varias posibilidades", y son la base de cómo se modelan los mensajes del protocolo: `enum ClientMessage { Join(String), Input(Direction), Interact, Leave }`.

### `Arc<Mutex<T>>`
Para compartir datos entre hilos de forma segura:
- `Mutex<T>` asegura que solo un hilo a la vez pueda leer/modificar el valor de adentro (evita condiciones de carrera).
- `Arc<T>` ("Atomic Reference Counted") permite que varios hilos tengan una referencia al mismo dato sin copiarlo, contando cuántas referencias existen para liberarlo cuando ya no se usa.
- Juntos, `Arc<Mutex<T>>` es el patrón estándar en Rust para "varios hilos necesitan leer y escribir el mismo estado compartido" — que es exactamente el caso del estado del juego (varios hilos de conexión de clientes, todos tocando el mismo `GameState`).

---

## 3. Redes: TCP vs UDP, por qué se usa cada uno

### TCP (puerto 5000) — para toda la partida
TCP es un protocolo **orientado a conexión**, **confiable** y **ordenado**:
- Antes de mandar datos, cliente y servidor hacen un "handshake" (se ponen de acuerdo en que ambos están ahí).
- Cada paquete que se manda, se garantiza que llegue, y en el mismo orden en que se mandó (si se pierde, se reenvía automáticamente).
- Es más "pesado" que UDP (más overhead), pero para un juego donde importa que "Join" llegue antes que "Input", y que ningún mensaje se pierda (imaginate perder el mensaje de "gané la partida"), es la elección correcta.

**Framing con prefijo de longitud**: como TCP es un flujo continuo de bytes (no sabe dónde termina un mensaje y empieza el siguiente), el protocolo agrega un prefijo `u16` (2 bytes) al principio de cada mensaje que dice "los próximos N bytes son un mensaje completo". Así el que recibe sabe exactamente dónde cortar.

### UDP (puerto 5001) — solo para discovery
UDP es **sin conexión** y **no garantiza nada**: un paquete puede perderse, llegar duplicado, o llegar desordenado, y a nadie le importa porque no hay confirmación de entrega.

¿Por qué usar algo tan poco confiable? Porque es perfecto para **broadcast** ("mandale esto a todos los que estén escuchando en la red, no sé quiénes son"). TCP no puede hacer broadcast (necesita saber la IP exacta del otro lado de antemano); UDP sí. Por eso el discovery ("¿hay algún servidor por acá?") usa UDP: el cliente grita "¿hay servidor?" a `255.255.255.255:5001`, y cualquier servidor que esté escuchando responde directamente a quien preguntó. Si el mensaje se pierde, no pasa nada grave — simplemente no vas a ver ese servidor en la lista, y podés reintentar apretando "Buscar" de nuevo.

### Por qué el discovery es más frágil en redes reales
El broadcast UDP depende de **en qué interfaz de red sale el paquete**. Una PC con Wi-Fi + Ethernet + adaptador virtual de VPN tiene varias "puertas de salida" posibles, y el sistema operativo elige una según su tabla de rutas (normalmente la de "ruta por defecto" hacia internet, no necesariamente la de la VPN). Por eso a veces el discovery funciona sobre RadminVPN y a veces no — depende de cómo esté priorizada esa interfaz en esa máquina específica en ese momento.

---

## 4. El protocolo PRFC-CC8-2026

Es la especificación que define exactamente qué mensajes existen y cómo se codifican en bytes.

### Mensajes Cliente → Servidor
| Mensaje | Qué significa |
|---|---|
| `Join(nombre)` | "Quiero entrar a la partida con este nombre" |
| `Input(dirección)` | "Quiero moverme en esta dirección" (una de 4: arriba/abajo/izq/der) |
| `Interact` | "Quiero agarrar/robar la bandera" (tecla E) |
| `Leave` | "Me voy de la partida" |

### Mensajes Servidor → Cliente
| Mensaje | Qué significa |
|---|---|
| `JoinAccepted` / `JoinRejected` | Si tu Join fue aceptado (nombre válido, cupo disponible) o no |
| `LobbyState` | Lista actual de jugadores esperando en el lobby |
| `GameCountdown` | Cuenta regresiva antes de que arranque la partida |
| `GameStarted` | La partida arrancó, empieza a simularse |
| `GameState` | Snapshot del estado actual (posiciones de todos, dónde está la bandera) — se manda repetidamente, ~20 veces por segundo |
| `FlagPickedUp` / `FlagStolen` | Alguien agarró la bandera del suelo / se la robó a otro jugador |
| `PlayerDisconnected` | Un jugador se cayó de la partida |
| `GameOver` | Alguien ganó |
| `Error` | Algo salió mal (por ejemplo, nombre inválido) |

### Codificación binaria
No se usa JSON ni ningún formato de texto — todo se codifica manualmente en binario:
- **Big-endian**: los números multi-byte se escriben con el byte más significativo primero (es una convención, tiene que coincidir en ambos lados o los números se leen mal).
- **Strings**: se codifican como 1 byte de longitud + los bytes UTF-8 del texto (por eso los nombres están limitados a 255 caracteres en teoría, y a 20 por regla del juego).
- Ventaja de esto sobre JSON: mensajes más chicos y más rápidos de parsear — importante porque `GameState` se manda 20 veces por segundo a cada cliente.

---

## 5. Modelo cliente-servidor autoritativo

Este es el concepto central del diseño, y probablemente lo que más te van a preguntar:

**¿Por qué el cliente no puede simplemente mover su propio personaje y avisarle al servidor "ahora estoy acá"?**

Porque eso permitiría hacer trampa fácilmente (un cliente modificado podría decir "estoy en cualquier posición" o "ya gané"). En cambio:

1. El cliente manda **intención**: "quiero moverme hacia arriba" (`Input`).
2. El servidor **simula** el juego: aplica ese input a la posición real que tiene guardada, revisa colisiones, revisa si con ese movimiento tocó la bandera, etc.
3. El servidor **transmite el resultado** (`GameState`) a todos los clientes.
4. El cliente **solo dibuja** lo que el servidor le dijo — no tiene autoridad para decidir nada por su cuenta.

Esto también explica por qué existe el concepto de **tick**: el servidor no simula "en tiempo continuo", sino en pasos discretos (cada 50ms = 20 veces por segundo). Cada tick, el servidor: lee los inputs pendientes → actualiza posiciones → revisa reglas del juego (bandera, victoria) → manda el `GameState` resultante con un número de tick que va incrementando.

### El bug de "congelamiento en la 2ª partida"
Este bug es un ejemplo perfecto de por qué el concepto de tick importa: el cliente estaba comparando "¿este `GameState` nuevo tiene un tick mayor al último que vi?" para descartar mensajes viejos que llegaran desordenados (recordá, TCP los ordena, pero por las dudas). El problema era que al arrancar una segunda partida, el servidor reinicia su contador de tick a 0, pero el cliente no reseteaba el suyo — entonces el cliente seguía esperando "tick > 300" (el último que había visto en la partida anterior) y descartaba todos los `GameState` nuevos hasta que el servidor volviera a llegar a ese número.

---

## 6. El game loop y los ticks

Un **game loop** es el patrón central de cualquier videojuego: un bucle infinito que corre mientras el juego está activo, haciendo repetidamente:

```
loop {
    procesar_input()
    actualizar_estado()
    dibujar_pantalla()
}
```

En este proyecto hay **dos loops distintos** corriendo en simultáneo, en máquinas distintas:

- **El loop del servidor** (autoritativo, a 20Hz / cada 50ms): no dibuja nada, solo simula el estado del juego y lo transmite.
- **El loop del cliente** (macroquad, normalmente a 60Hz o el refresh rate de tu monitor): lee tu teclado, manda inputs al servidor, y dibuja en pantalla el último `GameState` que recibió.

Como corren a velocidades distintas y en máquinas distintas, el cliente **no** dibuja exactamente "en tiempo real" lo que pasa — dibuja el último snapshot que le llegó por red, con la latencia que haya entre ambas máquinas. Esto es normal y esperable en cualquier juego en red.

---

## 7. macroquad: gráficos y game loop

Macroquad es una librería de Rust para hacer juegos 2D (y algo de 3D) de forma simple, sin necesidad de configurar un motor de juego completo tipo Unity o Godot.

Conceptos clave que vas a ver en el código:

- **`#[macroquad::main("Título")] async fn main()`**: el punto de entrada especial de macroquad — reemplaza al `fn main()` normal de Rust porque macroquad necesita manejar la ventana y el loop de renderizado internamente.
- **`loop { ... next_frame().await; }`**: el game loop de macroquad — cada vuelta del loop dibuja un frame nuevo, y `next_frame().await` espera hasta el próximo frame (sincronizado con el refresh de la pantalla).
- **`is_key_down(KeyCode::W)`**: revisa si una tecla está apretada en este frame — así se capturan los inputs de movimiento (WASD/flechas).
- **`draw_circle`, `draw_rectangle`, `draw_text`**: funciones para dibujar formas simples — el círculo central de la bandera, el punto naranja, el texto de la UI, todo se dibuja así, frame a frame, no son elementos de UI persistentes como en Slint.

Diferencia clave con Slint (que se dejó de usar): en macroquad **vos redibujás todo cada frame** (modo "immediate mode" — dibujás desde cero 60 veces por segundo), mientras que en Slint la UI es declarativa y persistente (describís cómo se ve, y el framework decide cuándo redibujar). Por eso macroquad encaja mejor con un juego con movimiento continuo.

---

## 8. Concurrencia: hilos, mutex, y por qué importa el orden de bloqueo

El servidor tiene que atender a **muchos clientes al mismo tiempo**. La forma típica de hacer esto es: por cada cliente que se conecta, se lanza un **hilo (thread)** dedicado a leer los mensajes de ese cliente específico.

El problema: todos esos hilos necesitan leer y escribir el **mismo** estado del juego (por ejemplo, la lista de jugadores, la posición de la bandera). Si dos hilos intentan modificar lo mismo al mismo tiempo sin coordinación, podés terminar con datos corruptos o inconsistentes (condición de carrera / *race condition*).

La solución es el **Mutex** ("mutual exclusion"): antes de tocar el dato compartido, un hilo tiene que "pedir el candado" (`lock()`). Mientras lo tiene, ningún otro hilo puede tocar ese dato — tiene que esperar a que se libere.

### El riesgo del deadlock
Si tenés **dos** mutex distintos (por ejemplo, uno para el estado de la sesión/partida y otro para el mapa de peers/conexiones), y distintas partes del código los bloquean **en orden distinto**, podés terminar en un **deadlock**: el hilo A tiene el candado de sesión y espera el de peers; el hilo B tiene el candado de peers y espera el de sesión — ninguno de los dos puede avanzar nunca, y el programa se cuelga.

La regla de oro es: **siempre bloqueá los mutex en el mismo orden en todo el código** (por ejemplo, siempre primero `session`, después `peers`, nunca al revés). Esto fue revisado específicamente en tu proyecto y confirmado como correcto — es una de las cosas buenas del diseño actual.

---

## 9. RadminVPN y por qué el discovery es un caso especial

RadminVPN crea una **red virtual** (una LAN simulada sobre internet) asignándole a cada máquina conectada una IP en el rango `26.x.x.x`. Para el sistema operativo de cada PC, esto aparece como una tarjeta de red más (un "adaptador virtual"), además de las que ya tenías (Wi-Fi, Ethernet).

- **TCP funciona sin problema** sobre esto: cuando le decís al cliente "conectate a 26.199.242.82:5000", el sistema operativo sabe exactamente por qué interfaz mandar esos paquetes (la ruta hacia esa IP específica pasa por el adaptador de Radmin), sin ambigüedad.
- **El broadcast UDP es ambiguo**: `255.255.255.255` no es una IP específica, es "todas las máquinas de mi red local" — pero ¿cuál red local, si tenés varias tarjetas? El sistema operativo tiene que elegir una, y normalmente prioriza la de tu ruta por defecto a internet (tu Wi-Fi/Ethernet real), no la VPN. Por eso el discovery es inconsistente: a veces funciona (si en esa máquina la métrica de la interfaz de Radmin quedó más prioritaria) y a veces no.

Esto es exactamente lo que viste en la práctica: la conexión manual siempre te funcionó, y el discovery automático fue "a veces sí, a veces no" según la máquina.

---

## 10. Preguntas típicas que te pueden hacer (y cómo responderlas)

**¿Por qué el servidor es autoritativo y no cada cliente controla su propio personaje?**
→ Para evitar trampas y mantener consistencia: todos los clientes ven la misma verdad porque hay una sola fuente de verdad (el servidor), en vez de que cada uno calcule su propia versión de la realidad.

**¿Por qué TCP para el juego y UDP para el discovery?**
→ TCP porque necesitamos que ningún mensaje se pierda ni llegue desordenado (perder un "gané la partida" sería grave). UDP para discovery porque necesitamos broadcast (mandarle a "cualquiera que esté escuchando"), algo que TCP no puede hacer, y perder un paquete de discovery no tiene consecuencias graves (simplemente reintentás).

**¿Qué es un tick y por qué importa?**
→ Es un paso discreto de la simulación del servidor (en este caso, cada 50ms = 20 veces por segundo). El servidor no simula en tiempo continuo; avanza el juego en pasos fijos, y cada paso transmite un snapshot numerado a los clientes.

**¿Qué pasa si dos jugadores agarran la bandera en el mismo instante?**
→ El servidor resuelve esto comparando contra el **estado "vivo" actual** del flag (no contra una copia vieja/snapshot), evitando que ambos "ganen" la carrera al mismo tiempo — es una prevención de condición de carrera a nivel de lógica de juego, no solo a nivel de threads.

**¿Por qué usan Mutex y no otra cosa?**
→ Porque varios hilos (uno por cada cliente conectado) necesitan leer y modificar el mismo estado compartido del juego, y el Mutex es la herramienta estándar de Rust para garantizar que solo uno lo toque a la vez, evitando corrupción de datos.

**¿Por qué no usan JSON para los mensajes?**
→ Por eficiencia: el protocolo binario manual es más chico y rápido de parsear que JSON, lo cual importa porque `GameState` se manda muchas veces por segundo a cada cliente conectado.

**¿Por qué el discovery automático no siempre funciona por RadminVPN?**
→ Porque el broadcast UDP depende de qué interfaz de red el sistema operativo elige como default, y en máquinas con múltiples adaptadores (Wi-Fi/Ethernet + VPN) eso no siempre coincide con el adaptador de la VPN. La conexión manual por IP:puerto no tiene este problema porque usa TCP con una IP de destino específica, sin ambigüedad de ruteo.
