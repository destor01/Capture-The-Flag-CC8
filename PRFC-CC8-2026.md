# PRFC-CC8-2026 — Captura la Bandera

**Especificación funcional y protocolo de comunicación**

| | |
|---|---|
| Versión del documento | `3.0.0` |
| `protocolVersion` (byte) en los mensajes | `3` |
| Estado | Propuesto |
| Última modificación | 2026-07-23 |
| Reemplaza a | `2.0.x` (transporte JSON, movimiento vectorial), incompatible |

> El número de versión del documento y el byte de versión que viaja en los
> mensajes **no son lo mismo**. El documento puede llegar a `3.0.7` por
> aclaraciones de redacción mientras los mensajes siguen usando el byte `3`. El
> byte solo cambia cuando se rompe la compatibilidad.

## Sobre esta versión

Cambia dos cosas respecto de la 2.0:

- **Transporte binario en vez de JSON.** Los mensajes viajan como bytes, no como
  texto. Un `GAME_STATE` de dos jugadores pasa de ~400 bytes a 50.
- **Movimiento en 4 direcciones en vez de vectorial.** El reglamento habla de
  "desplazarse libremente utilizando los controles del teclado" y no menciona
  diagonales. Se implementa como arriba, abajo, izquierda y derecha, que es lo
  que dice el reglamento sin agregarle nada, y elimina la ambigüedad de cómo se
  normaliza una diagonal entre 13 implementaciones distintas.

El juego, los estados, la victoria y el descubrimiento son los mismos. Las citas
por sección de la 2.0 no se corresponden con las de esta.

Este documento es exhaustivo hasta el byte a propósito: con un formato binario,
las 13 implementaciones tienen que acertar el mismo layout, y un solo grupo que
lea un entero al revés queda incompatible con todos. Se recomienda conservar en
paralelo un modo de texto para depurar (§37).

Las secciones se citan por número: §14 (robo de la bandera), §29.6 (mensaje
`GAME_STATE`).

---

## 1. Propósito

Este documento define las reglas del juego, el funcionamiento del servidor y el
protocolo de comunicación que deberán respetar todos los proyectos del curso.

Cada proyecto podrá utilizar cualquier lenguaje de programación, librería, motor
gráfico o interfaz, siempre que cumpla exactamente con las reglas y mensajes
definidos aquí. Todos los proyectos deben poder conectarse entre sí.

## 2. Objetivo del juego

Cada jugador compite de forma individual por una única bandera. Para ganar, un
jugador debe:

1. Entrar al círculo central.
2. Tomar la bandera.
3. Salir completamente del círculo llevándola.

El primer jugador que lo consiga gana la partida.

Tomar la bandera no significa ganar. Es obligatorio salir del círculo con ella.

## 3. Arquitectura general

El juego utilizará una arquitectura cliente-servidor.

- Existirá un único servidor por partida.
- Todos los jugadores se conectarán directamente al servidor.
- Los jugadores no se comunicarán directamente entre sí.
- El servidor mantendrá el estado oficial del juego.
- Los clientes únicamente enviarán intención de movimiento e interacción.
- El servidor validará todas las acciones.

## 4. Modos del proyecto

Cada proyecto deberá poder ejecutarse en dos modos.

**Modo servidor.** Hospeda la partida, mantiene el estado oficial y **únicamente
muestra** el juego de todos los jugadores conectados. La máquina que corre el
servidor **no participa como jugador**: no tiene entidad en el mapa, no aparece
en la lista de jugadores y no puede tomar la bandera.

**Modo cliente.** Se conecta a un servidor y es el único modo en el que se juega
desde esa máquina.

Un proyecto deberá poder conectarse al servidor de cualquier otro proyecto, y su
servidor deberá aceptar clientes de cualquier otro proyecto.

## 5. El mapa

El mapa será un plano continuo, no una rejilla.

- Las coordenadas se expresan en **unidades de mundo**, con dos decimales.
- El origen `(0, 0)` es el centro del mapa y también el centro del círculo.
- El eje **x** crece hacia la derecha.
- El eje **y** crece hacia **abajo**, siguiendo la convención de pantalla.

El mapa es un cuadrado de `mapSize` × `mapSize` unidades centrado en el origen.
Las coordenadas válidas van de `-mapSize / 2` a `+mapSize / 2` en ambos ejes.

Ningún jugador podrá salir del mapa: el servidor recorta la posición a esos
límites.

Que el mapa sea continuo y el movimiento de 4 direcciones no se contradice: el
jugador se mueve en pasos pequeños en una de las cuatro direcciones, no salta de
casilla en casilla. La conversión de unidades de mundo a píxeles es decisión de
cada cliente.

## 6. El círculo central

En el centro del mapa existe un área circular de radio `circleRadius`, centrada
en el origen.

Un jugador se considera **dentro del círculo** cuando la distancia de su centro
al origen es menor o igual a `circleRadius`.

Un jugador se considera **completamente fuera del círculo** cuando:

```
distancia(jugador, origen) - playerRadius > circleRadius
```

Esa distinción importa solo para la condición de victoria (§16). No basta con
tocar el borde.

El círculo no es una barrera: cualquiera puede entrar y salir libremente en
cualquier momento, tenga o no la bandera.

## 7. La bandera

Existirá una única bandera, ubicada exactamente en el centro del mapa, en `(0, 0)`.

Estados posibles:

| Estado | Código (§25) | Significado |
|---|---|---|
| `AVAILABLE` | `0x01` | Está en el suelo y nadie la lleva. |
| `CARRIED` | `0x02` | Un jugador la transporta. |
| `DROPPED` | `0x03` | Cayó porque su portador se desconectó. |
| `OUTSIDE` | `0x04` | Salió del círculo y la partida terminó. |

Mientras está `CARRIED`, la posición de la bandera es la del portador.

Mientras está `DROPPED`, permanece fija en el punto donde cayó y puede volver a
tomarse.

## 8. Jugadores

El servidor aceptará hasta `maximumPlayers` jugadores simultáneos.

Cada jugador tendrá:

| Campo | Tipo | Descripción |
|---|---|---|
| `playerId` | u16 | Identificador único asignado por el servidor. El `0` significa "ninguno". |
| `name` | str | Nombre visible del jugador. |
| `x` | i32 | Posición horizontal (unidades × 100, ver §24). |
| `y` | i32 | Posición vertical (unidades × 100). |
| `direction` | u8 | Dirección activa (§10). |
| `hasFlag` | bool | Indica si lleva la bandera. |

No existe campo de protección ni de inmunidad. El reglamento no las contempla
(§14).

## 9. Posición inicial

Cada jugador aparecerá en una posición aleatoria **fuera del círculo**.

El servidor elegirá un ángulo aleatorio y colocará al jugador a una distancia del
origen de `circleRadius + spawnMargin`, dentro de los límites del mapa.

Todos los jugadores comienzan quietos (`direction = NONE`). Ningún jugador
comienza con la bandera.

## 10. Movimiento

El movimiento es en cuatro direcciones. No hay diagonales.

El reglamento del curso indica que los jugadores se desplazan libremente con los
controles del teclado y no menciona movimiento diagonal. Este protocolo lo
implementa como cuatro direcciones para no introducir una regla que el reglamento
no define y que cada implementación podría interpretar distinto.

Direcciones y su código:

| Dirección | Código | Efecto por ciclo |
|---|---|---|
| `NONE` | `0x00` | El jugador está quieto. |
| `UP` | `0x01` | `y` disminuye (el eje `y` crece hacia abajo, §5). |
| `DOWN` | `0x02` | `y` aumenta. |
| `LEFT` | `0x03` | `x` disminuye. |
| `RIGHT` | `0x04` | `x` aumenta. |

Reglas:

- El cliente envía su **dirección activa** con un mensaje `INPUT` (§28.2). Esa
  dirección se mantiene vigente hasta que envíe otra; el cliente no necesita
  reenviarla en cada ciclo.
- `NONE` significa quieto. Detenerse es una acción válida.
- El cliente **nunca** envía posiciones. Solo la dirección.

En cada ciclo el servidor mueve a cada jugador un paso en su dirección activa:

```
paso = playerSpeed * tickIntervalMs / 1000
```

Por ejemplo, con dirección `RIGHT`, `x` aumenta en `paso`; con `UP`, `y`
disminuye en `paso`. Después el servidor recorta la posición a los límites del
mapa.

Los jugadores **no colisionan entre sí**. Pueden ocupar el mismo punto. La
cercanía solo importa para interactuar (§12).

## 11. Ciclo del servidor

El servidor ejecutará ciclos automáticos. Intervalo recomendado:
`tickIntervalMs`, con valor inicial de **50 milisegundos**, es decir 20 ciclos
por segundo.

Cada ciclo tendrá un número entero consecutivo, `tick`, que comienza en `1`.

Al final de cada ciclo el servidor envía `GAME_STATE` a todos los clientes.

## 12. Interacción

Tomar y robar la bandera requieren que el jugador **presione la tecla de
interacción**. Nunca ocurren de forma automática por acercarse.

Cuando el jugador la presiona, el cliente envía un mensaje `INTERACT` (§28.3). La
tecla concreta es decisión de cada proyecto.

El servidor procesará la interacción si la distancia entre el jugador y el
objetivo es menor o igual a `interactionRadius`.

Interactuar y moverse son mensajes distintos e independientes: un jugador puede
estar moviéndose y robar en el mismo instante. Por eso la dirección va en el
`INPUT` y la acción de interactuar en su propio `INTERACT`.

Si el jugador envía varios `INTERACT` antes del mismo ciclo, el servidor
procesará **únicamente uno**. Esto no es una inmunidad: es evitar que un cliente
que envía cien mensajes por segundo tenga ventaja sobre uno que envía veinte.

Si un `INTERACT` no cumple las condiciones —está lejos, la bandera ya la lleva él
mismo, no hay nada cerca— el servidor lo **ignora en silencio**. No envía `ERROR`.
Presionar la tecla sin resultado es parte normal del juego y responder con un
error inundaría la conexión.

## 13. Tomar la bandera

Si un jugador envía `INTERACT`, la bandera está en estado `AVAILABLE` o `DROPPED`,
y la distancia entre el jugador y la bandera es menor o igual a
`interactionRadius`:

- la bandera pasa inmediatamente a pertenecer a ese jugador;
- deja de estar en el suelo y pasa a estado `CARRIED`;
- desde ese momento acompaña al jugador.

El servidor envía `FLAG_PICKED_UP` (§29.7).

## 14. Robo de la bandera

Si un jugador posee la bandera, cualquier otro jugador puede robársela.

Condiciones:

- el atacante debe estar a una distancia del portador menor o igual a
  `interactionRadius`;
- el atacante debe enviar `INTERACT`.

Si ambas se cumplen, la bandera cambia inmediatamente de propietario. Ninguno de
los dos cambia de posición y ninguno es eliminado.

**No existe tiempo de espera. No existe inmunidad. El robo es instantáneo.**

La bandera podrá cambiar de dueño tantas veces como haga falta durante la
partida, incluso en ciclos consecutivos.

El servidor envía `FLAG_STOLEN` (§29.8).

## 15. Conflictos simultáneos

Todas las interacciones de un ciclo se evalúan contra el mismo estado inicial.

Si varios jugadores envían `INTERACT` sobre el portador en el mismo ciclo:

- el servidor los ordena por `playerId` ascendente;
- **únicamente el primer robo válido se aplica**;
- los demás no producen ningún efecto.

Quien quiera robarle al nuevo portador deberá presionar la tecla otra vez en un
ciclo posterior. El orden por `playerId` debe ser estable para que todos los
servidores resuelvan igual el mismo empate.

## 16. Condición de victoria

Un jugador gana cuando:

1. tiene la bandera; y
2. cruza **completamente** el límite del círculo hacia el exterior, es decir
   `distancia(jugador, origen) - playerRadius > circleRadius`.

No basta con tocar el borde. Debe encontrarse totalmente fuera del área central.

El servidor evalúa esta condición al final de cada ciclo, después de mover a los
jugadores. Cuando se cumple:

- el estado de la partida cambia a `FINISHED`;
- la bandera cambia a estado `OUTSIDE`;
- no se procesan más movimientos ni interacciones;
- el servidor envía el `GAME_STATE` final y después `GAME_OVER` (§29.11).

## 17. Desconexiones

Si un jugador se desconecta:

- será eliminado del mapa;
- dejará de aparecer en la lista de jugadores a partir del siguiente `GAME_STATE`;
- el servidor notificará a los demás con `PLAYER_DISCONNECTED` (§29.9).

Si el jugador llevaba la bandera:

- la bandera caerá en su última posición válida;
- cambiará a estado `DROPPED`;
- podrá ser recogida por cualquier jugador con `INTERACT` (§13).

No habrá reconexión automática.

## 18. Estados de la partida

| Estado | Código (§25) | Significado |
|---|---|---|
| `WAITING` | `0x01` | Acepta jugadores. Aparece en el descubrimiento. |
| `STARTING` | `0x02` | Cuenta regresiva en curso. Ya no acepta jugadores. |
| `RUNNING` | `0x03` | La partida está en juego. |
| `FINISHED` | `0x04` | Existe un ganador. |
| `CANCELLED` | `0x05` | El servidor canceló la partida. |

No se aceptarán jugadores después de que inicie la cuenta regresiva.

## 19. Descubrimiento de servidores

El cliente deberá poder **descubrir servidores disponibles** sin que el usuario
escriba una dirección IP.

El descubrimiento usa **UDP broadcast** en `discoveryPort`. Es el único uso de
UDP en el protocolo: la partida completa viaja por TCP (§22).

Flujo:

1. El cliente envía por broadcast un `DISCOVER_REQUEST` (§27.1) a
   `255.255.255.255:discoveryPort`.
2. Todo servidor en estado `WAITING` responde por UDP directo al remitente con un
   `DISCOVER_RESPONSE` (§27.2), que incluye su puerto TCP.
3. El cliente muestra la lista y el usuario elige.
4. El cliente abre una conexión TCP al servidor elegido y envía `JOIN` (§28.1).

Un servidor que no esté en `WAITING` no responde. El cliente deberá permitir
también la conexión manual por IP y puerto, para redes donde el broadcast esté
bloqueado.

## 20. Inicio de la partida

El anfitrión —quien ejecuta el modo servidor— controla el inicio.

Flujo:

1. El servidor entra en estado `WAITING` y comienza a responder el descubrimiento.
2. Los clientes envían `JOIN`; el servidor acepta o rechaza cada conexión.
3. Cada vez que la lista de jugadores cambia, el servidor envía `LOBBY_STATE`
   (§29.3) a todos.
4. El anfitrión inicia la partida.
5. El servidor cambia a `STARTING` y envía `GAME_COUNTDOWN` (§29.4) una vez por
   segundo, desde `countdownSeconds` hasta `1`.
6. Al terminar la cuenta, genera las posiciones iniciales y envía `GAME_STARTED`
   (§29.5).
7. Cambia a `RUNNING` y comienza a enviar `GAME_STATE`.

## 21. Parámetros configurables

| Parámetro | Valor inicial | Descripción |
|---|---|---|
| `mapSize` | 2000 | Lado del mapa cuadrado, en unidades de mundo. |
| `circleRadius` | 500 | Radio del círculo central. |
| `playerRadius` | 15 | Radio del jugador, usado en la condición de victoria. |
| `spawnMargin` | 80 | Distancia extra fuera del círculo donde aparecen los jugadores. |
| `playerSpeed` | 220 | Unidades de mundo por segundo. |
| `interactionRadius` | 60 | Alcance de la tecla de interacción. |
| `tickIntervalMs` | 50 | Duración del ciclo del servidor. |
| `countdownSeconds` | 5 | Duración de la cuenta regresiva. |
| `maximumPlayers` | 100 | Máximo de jugadores simultáneos. |
| `serverPort` | 5000 | Puerto TCP de la partida. |
| `discoveryPort` | 5001 | Puerto UDP del descubrimiento. |

Los valores efectivos viajan en `GAME_STARTED`. El cliente **no** debe asumir los
valores por defecto: debe leer los que le manda el servidor.

## 22. Protocolo de transporte

| Uso | Transporte | Puerto |
|---|---|---|
| Descubrimiento de servidores | UDP broadcast | `discoveryPort` |
| Partida completa | TCP | `serverPort` |

TCP para la partida porque garantiza entrega y orden, permite detectar
desconexiones y está disponible en todos los lenguajes.

UDP únicamente para el descubrimiento, porque el broadcast lo requiere y la
pérdida de un datagrama de descubrimiento no tiene consecuencias: el cliente
vuelve a preguntar.

No se utilizará WebSocket. Un cliente que corra dentro de un navegador necesitará
un proceso puente que hable TCP y UDP del lado del sistema operativo.

## 23. Formato de comunicación

Los mensajes viajan como **bytes**, no como texto.

### 23.1 Tipos base

Todos los enteros de más de un byte van en **big-endian** (el byte más
significativo primero), que es el orden de red. En Java y las APIs de sockets es
el predeterminado; en C#, Go y Rust hay que pedirlo. Es el error más común: si
algo no conecta, revisen esto primero.

| Nombre | Bytes | Rango | Notas |
|---|---|---|---|
| `u8` | 1 | 0 a 255 | entero sin signo |
| `u16` | 2 | 0 a 65 535 | big-endian |
| `u32` | 4 | 0 a 4 294 967 295 | big-endian |
| `i16` | 2 | −32 768 a 32 767 | complemento a dos, big-endian |
| `i32` | 4 | −2 147 483 648 a 2 147 483 647 | complemento a dos, big-endian |
| `str` | 1 + N | — | un `u8` de longitud, luego N bytes UTF-8 |
| `bool` | 1 | 0 o 1 | cualquier valor distinto de 0 se lee como verdadero |

No se usan números de punto flotante. Todas las coordenadas son enteros (§24).

### 23.2 Enmarcado sobre TCP

El flujo TCP no tiene fronteras de mensaje, así que cada mensaje va precedido por
su longitud:

```
+--------+--------------------------+
| u16 N  | cuerpo del mensaje (N)   |
+--------+--------------------------+
```

El receptor lee 2 bytes (obtiene N), luego lee **exactamente** N bytes, y repite.
"Exactamente N" importa: TCP puede entregar la lectura partida en varios pedazos,
hay que insistir hasta juntar N.

El máximo de `u16` deja mensajes de hasta 65 535 bytes, de sobra para un
`GAME_STATE` de 100 jugadores (~1 500 bytes).

### 23.3 Sobre UDP

Cada datagrama es un mensaje completo, sin prefijo de longitud.

### 23.4 Encabezado común

Todo mensaje empieza con dos bytes:

```
+------+------+ ...
| u8   | u8   |
| tipo | ver  |
+------+------+
```

- **tipo**: identifica el mensaje, según la tabla de §26.
- **ver**: versión del protocolo, `3`. El receptor rechaza cualquier otro valor
  con un `ERROR` de código `UNSUPPORTED_PROTOCOL_VERSION` (§29.12).

El `gameId` **no viaja** en los mensajes TCP: una conexión TCP pertenece a una
sola partida. Solo aparece en el descubrimiento UDP.

## 24. Coordenadas

Las coordenadas del mundo tienen dos decimales. En binario se mandan como el
entero de multiplicar por 100:

```
valor transmitido = round(coordenada × 100)   como i32
```

Ejemplo: `x = -120.75` viaja como `i32` con valor `-12075`. El receptor divide
por 100 al leer.

Se usa `i32` porque un mapa de 2000 unidades llega a ±1000, que por 100 son
±100 000, muy por encima del tope de `i16`.

## 25. Enumeraciones

Los estados y motivos que antes eran texto pasan a un `u8`.

- **Estado de la partida**: tabla de §18.
- **Estado de la bandera**: tabla de §7.
- **Dirección**: tabla de §10.
- **Motivo de `JOIN_REJECTED`**: `GAME_ALREADY_STARTED` = `0x01`, `GAME_FULL` =
  `0x02`, `INVALID_NAME` = `0x03`, `UNSUPPORTED_PROTOCOL_VERSION` = `0x04`.
- **Motivo de `GAME_OVER`**: `EXITED_CIRCLE_WITH_FLAG` = `0x01`.
- **Código de `ERROR`**: tabla de §29.12.

## 26. Tabla de tipos de mensaje

| Código | Mensaje | Dirección |
|---|---|---|
| `0x01` | `DISCOVER_REQUEST` | cliente → broadcast (UDP) |
| `0x02` | `DISCOVER_RESPONSE` | servidor → cliente (UDP) |
| `0x10` | `JOIN` | cliente → servidor |
| `0x11` | `INPUT` | cliente → servidor |
| `0x12` | `INTERACT` | cliente → servidor |
| `0x13` | `LEAVE` | cliente → servidor |
| `0x20` | `JOIN_ACCEPTED` | servidor → cliente |
| `0x21` | `JOIN_REJECTED` | servidor → cliente |
| `0x22` | `LOBBY_STATE` | servidor → cliente |
| `0x23` | `GAME_COUNTDOWN` | servidor → cliente |
| `0x24` | `GAME_STARTED` | servidor → cliente |
| `0x25` | `GAME_STATE` | servidor → cliente |
| `0x26` | `FLAG_PICKED_UP` | servidor → cliente |
| `0x27` | `FLAG_STOLEN` | servidor → cliente |
| `0x28` | `PLAYER_DISCONNECTED` | servidor → cliente |
| `0x29` | `GAME_OVER` | servidor → cliente |
| `0x2A` | `ERROR` | servidor → cliente |

## 27. Mensajes de descubrimiento (UDP)

### 27.1 DISCOVER_REQUEST (0x01)

Solo el encabezado. 2 bytes.

```
01 03
```

### 27.2 DISCOVER_RESPONSE (0x02)

```
u8   tipo = 0x02
u8   ver  = 0x03
u16  gameId
str  serverName
u16  tcpPort
u8   state           (§18)
u16  playerCount
u16  maximumPlayers
```

La IP del servidor no viaja en el mensaje: el cliente la obtiene del datagrama
recibido, porque un servidor con varias interfaces no sabe cuál ve el cliente.

## 28. Mensajes del cliente al servidor (TCP)

### 28.1 JOIN (0x10)

```
u8   tipo = 0x10
u8   ver  = 0x03
str  name        (1 a 20 bytes UTF-8)
```

El nombre debe tener entre 1 y 20 caracteres tras quitar espacios. Fuera de ese
rango, el servidor responde `JOIN_REJECTED` con `INVALID_NAME`. Los nombres no
tienen que ser únicos; la identidad es el `playerId`.

### 28.2 INPUT (0x11)

Actualiza la dirección activa. Se envía cuando cambia, no en cada cuadro.

```
u8   tipo = 0x11
u8   ver  = 0x03
u16  playerId
u8   direction   (§10)
```

**5 bytes.** Ejemplo, P07 moviéndose hacia arriba (`direction = UP = 0x01`):

```
11 03 00 07 01
```

Si el cliente envía varios `INPUT` antes del mismo ciclo, solo se aplica el
último.

### 28.3 INTERACT (0x12)

El jugador presionó la tecla de interacción.

```
u8   tipo = 0x12
u8   ver  = 0x03
u16  playerId
```

**4 bytes.** El cliente no indica el objetivo; el servidor decide qué corresponde
según §13 y §14.

### 28.4 LEAVE (0x13)

El jugador abandona voluntariamente.

```
u8   tipo = 0x13
u8   ver  = 0x03
u16  playerId
```

## 29. Mensajes del servidor al cliente (TCP)

### 29.1 JOIN_ACCEPTED (0x20)

```
u8   tipo = 0x20
u8   ver  = 0x03
u16  playerId    (el asignado a este cliente)
u16  gameId
```

### 29.2 JOIN_REJECTED (0x21)

```
u8   tipo = 0x21
u8   ver  = 0x03
u8   reason      (§25)
```

### 29.3 LOBBY_STATE (0x22)

El patrón para toda lista de longitud variable es un `u8` con la cantidad,
seguido de esa cantidad de bloques.

```
u8   tipo = 0x22
u8   ver  = 0x03
u8   state           (§18)
u8   count
count × {
  u16  playerId
  str  name
}
```

### 29.4 GAME_COUNTDOWN (0x23)

```
u8   tipo = 0x23
u8   ver  = 0x03
u8   secondsRemaining
```

### 29.5 GAME_STARTED (0x24)

```
u8   tipo = 0x24
u8   ver  = 0x03
i32  mapSize × 100
i32  circleRadius × 100
i32  playerRadius × 100
i32  playerSpeed × 100
i32  interactionRadius × 100
u16  tickIntervalMs
u8   flagStatus       (§7)
u16  flagCarrierId    (0 = ninguno)
i32  flagX × 100
i32  flagY × 100
u8   count
count × {
  u16  playerId
  str  name
  i32  x × 100
  i32  y × 100
  u8   direction
  bool hasFlag
}
```

### 29.6 GAME_STATE (0x25)

El mensaje que más viaja: 20 veces por segundo a cada cliente. El bloque de
jugador aquí **no incluye `name`** —el cliente ya lo recibió en `GAME_STARTED` o
`LOBBY_STATE` y lo asocia por `playerId`—, lo que baja cada jugador a 12 bytes.

```
u8   tipo = 0x25
u8   ver  = 0x03
u32  tick
u8   flagStatus       (§7)
u16  flagCarrierId    (0 = ninguno)
i32  flagX × 100
i32  flagY × 100
u8   count
count × {
  u16  playerId
  i32  x × 100
  i32  y × 100
  u8   direction
  bool hasFlag
}
```

Con dos jugadores, el cuerpo ocupa **42 bytes** (2 encabezado + 4 tick + 11
bandera + 1 count + 2×12 jugadores), más 2 del prefijo de longitud: 44 en total. El mismo
mensaje en la v2.0 JSON eran ~400. Con 100 jugadores, el cuerpo son 1 218 bytes
contra ~12 300.

El cliente deberá mostrar siempre el estado con el `tick` más reciente e ignorar
los que lleguen con un `tick` menor al último recibido.

### 29.7 FLAG_PICKED_UP (0x26)

```
u8   tipo = 0x26
u8   ver  = 0x03
u32  tick
u16  playerId
```

### 29.8 FLAG_STOLEN (0x27)

```
u8   tipo = 0x27
u8   ver  = 0x03
u32  tick
u16  previousCarrierId
u16  newCarrierId
```

No lleva tiempo de protección: no existe (§14).

### 29.9 PLAYER_DISCONNECTED (0x28)

```
u8   tipo = 0x28
u8   ver  = 0x03
u16  playerId
```

### 29.10 GAME_OVER (0x29)

```
u8   tipo = 0x29
u8   ver  = 0x03
u16  winnerId
str  winnerName
u8   reason          (§25; siempre EXITED_CIRCLE_WITH_FLAG)
```

### 29.11 Orden de los mensajes en un mismo ciclo

Dentro de un ciclo, el servidor envía primero los eventos —`FLAG_PICKED_UP`,
`FLAG_STOLEN`, `PLAYER_DISCONNECTED`— y al final el `GAME_STATE` de ese `tick`.
Si hay varios eventos, van en orden ascendente de `playerId` del jugador afectado.

En el ciclo de la victoria se envía el `GAME_STATE` final y **después**
`GAME_OVER`. Sin ese último estado el cliente nunca dibujaría el momento en que
el ganador cruza el borde.

### 29.12 ERROR (0x2A)

```
u8   tipo = 0x2A
u8   ver  = 0x03
u8   code
str  description     (puede ir vacío: longitud 0)
```

Códigos:

| Valor | Código |
|---|---|
| `0x01` | `INVALID_MESSAGE` |
| `0x02` | `INVALID_ENCODING` |
| `0x03` | `INVALID_INPUT` |
| `0x04` | `UNKNOWN_PLAYER` |
| `0x05` | `GAME_NOT_STARTED` |
| `0x06` | `GAME_ALREADY_STARTED` |
| `0x07` | `GAME_FINISHED` |
| `0x08` | `UNSUPPORTED_PROTOCOL_VERSION` |

`INVALID_ENCODING` se envía cuando un mensaje no se puede decodificar: longitud
declarada que no cuadra, string que se sale del buffer, dirección o tipo
desconocidos.

Un `INTERACT` que no cumple condiciones **no** genera error (§12).

## 30. Funcionamiento interno del servidor

En cada ciclo, el servidor deberá:

1. recopilar los `INPUT` pendientes y conservar solo el último de cada jugador;
2. recopilar los `INTERACT` pendientes y conservar solo uno por jugador;
3. aplicar la dirección de cada jugador;
4. calcular la nueva posición de cada jugador;
5. recortar las posiciones a los límites del mapa;
6. resolver las interacciones en orden ascendente de `playerId`;
7. actualizar el portador y la posición de la bandera;
8. verificar desconexiones;
9. verificar la condición de victoria;
10. incrementar el `tick`;
11. enviar los eventos del ciclo y después el `GAME_STATE`.

## 31. Reglas de sincronización

- El servidor será la única fuente oficial.
- Los clientes no calcularán posiciones definitivas.
- Los clientes solo enviarán dirección e interacción.
- Todos los jugadores se evaluarán una vez por ciclo.
- Los movimientos e interacciones de un ciclo usarán el mismo estado inicial.
- Todos los clientes recibirán el mismo resultado.
- Un cliente deberá ignorar estados con un `tick` menor al último recibido.

Un cliente **puede** predecir localmente el movimiento de su propio jugador para
que se sienta fluido entre ciclos, siempre que corrija su posición cuando llegue
el `GAME_STATE`. Esa predicción es opcional y nunca es autoritativa.

## 32. Validaciones obligatorias del servidor

El servidor deberá validar:

- que el mensaje se decodifica sin errores;
- tipo de mensaje conocido;
- versión de protocolo compatible;
- jugador registrado;
- que el `playerId` corresponde a la conexión que envió el mensaje;
- dirección dentro de las cinco válidas (§10);
- partida en el estado correcto;
- límites del mapa;
- distancia para interactuar;
- condición de robo;
- condición de victoria.

Nunca deberá confiar en posiciones enviadas por el cliente. El cliente no tiene
ningún campo de posición: solo dirección e interacción.

## 33. Mensajes mínimos obligatorios

Descubrimiento (UDP): `DISCOVER_REQUEST`, `DISCOVER_RESPONSE`.

Cliente hacia servidor (TCP): `JOIN`, `INPUT`, `INTERACT`, `LEAVE`.

Servidor hacia cliente (TCP): `JOIN_ACCEPTED`, `JOIN_REJECTED`, `LOBBY_STATE`,
`GAME_COUNTDOWN`, `GAME_STARTED`, `GAME_STATE`, `FLAG_PICKED_UP`, `FLAG_STOLEN`,
`PLAYER_DISCONNECTED`, `GAME_OVER`, `ERROR`.

## 34. Compatibilidad entre lenguajes

Cada proyecto podrá usar cualquier lenguaje. Soporte de TCP, UDP y bytes:

| Lenguaje | TCP | UDP | Bytes big-endian |
|---|---|---|---|
| C# | `TcpClient` | `UdpClient` | `BinaryPrimitives` |
| Java | `Socket` | `DatagramSocket` | `DataInputStream` (ya es big-endian) |
| Python | `socket` | `socket` con `SO_BROADCAST` | `struct` con formato `>` |
| C/C++ | sockets del sistema | sockets del sistema | `htons` / `htonl` |
| Go | `net.Dial` | `net.ListenPacket` | `encoding/binary` BigEndian |
| Node.js | `net` | `dgram` | `Buffer.readUInt16BE` |
| Rust | `TcpStream` | `UdpSocket` | `from_be_bytes` |

## 35. Prueba mínima de compatibilidad

Antes de conectar con otro proyecto, cada implementación debería verificar contra
sí misma:

1. Serializar un `INPUT` de P07 hacia arriba y comprobar que da exactamente
   `11 03 00 07 01`. Es la prueba de oro: si estos 5 bytes no coinciden, no se
   interopera con nadie, y se sabe sin necesidad de otro grupo.
2. Serializar y deserializar cada mensaje y comprobar que vuelve igual.
3. Leer un `GAME_STATE` de otro emisor y comprobar que las coordenadas tienen
   sentido: valores como 30 000 donde esperabas 300 delatan el endianness
   invertido.
4. Enviar dos mensajes pegados y verificar que el receptor los separa por el
   prefijo de longitud, no por adivinanza.

Después, la prueba de extremo a extremo contra otro proyecto:

5. `DISCOVER_REQUEST` por broadcast y recepción de `DISCOVER_RESPONSE`.
6. Conexión TCP, `JOIN` y `JOIN_ACCEPTED`.
7. `LOBBY_STATE` al entrar otro jugador.
8. `GAME_COUNTDOWN` y `GAME_STARTED`.
9. `INPUT` y comprobación de que la posición cambia en el `GAME_STATE`.
10. `INTERACT` cerca de la bandera y recepción de `FLAG_PICKED_UP`.
11. Cierre correcto y recepción de `PLAYER_DISCONNECTED` del otro lado.

## 36. Resumen técnico

| | |
|---|---|
| Arquitectura | Cliente-servidor |
| Servidor | Único, no juega, solo muestra |
| Descubrimiento | UDP broadcast, puerto 5001 |
| Partida | TCP, puerto 5000 |
| Formato | Binario, big-endian |
| Enmarcado TCP | Prefijo de longitud u16 |
| Encabezado | u8 tipo + u8 versión |
| Versión (byte) | 3 |
| Mapa | Plano continuo de 2000 × 2000 unidades |
| Origen | Centro del mapa y del círculo |
| Ejes | `x` a la derecha, `y` hacia abajo |
| Círculo central | Radio 500 |
| Coordenadas | Enteros, unidades × 100 |
| Movimiento | 4 direcciones: UP, DOWN, LEFT, RIGHT |
| Velocidad | 220 unidades por segundo |
| Interacción | Tecla, alcance 60 unidades |
| Inmunidad | No existe |
| Ciclo | 50 ms, 20 por segundo |
| Jugadores | Hasta 100 |
| Estado oficial | Servidor |
| Lenguaje y librerías | Libres |

## 37. Recomendación: conservar un modo de texto para depurar

Depurar bytes es mucho más difícil que depurar texto. Se recomienda que cada
implementación soporte también un transporte de texto (el JSON de la v2.0, o
cualquier volcado legible) elegible con una bandera de arranque.

Cuando dos servidores no se entienden en binario, pasarlos a texto y comparar las
dos representaciones encuentra el problema en minutos. Sin ese modo, la única
herramienta es un volcado hexadecimal.

Los dos transportes no se mezclan en una misma conexión: se decide al abrir el
socket, no mensaje por mensaje.

Todos los proyectos deberán respetar esta especificación para garantizar que
puedan comunicarse entre sí.
