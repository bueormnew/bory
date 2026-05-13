# Guia De BORY (Espanol)

## Vision General

BORY busca una mezcla concreta:

- sintaxis simple
- menos ruido estructural
- herramientas rapidas
- un runtime que pueda seguir creciendo sin romper el estilo del lenguaje

En esta version se ampliaron varias bases del proyecto:

- bloques por indentacion opcionales
- contratos de tipos en variables, parametros, retornos y constructores
- VM de bytecode para expresiones
- inspeccion del heap con `gc`
- paquetes incluidos para pantalla, juegos y datos

## Sintaxis Central

### Variables

```boy
var nombre = "BORY"
var puntaje = 10
var listo = yes
var vacio = nil
```

### Variables Tipadas

```boy
var puntaje: number = 10
var nombre: text = "BORY"
var numeros: list[number] = [1, 2, 3]
```

Si el valor no coincide con el tipo declarado, el runtime produce un error con codigo `TYPE001`.

### Tareas

```boy
task sumar(a: number, b: number) -> number =>
    give a + b
```

### Bloques Sin `end`

```boy
if puntaje > 50 =>
    echo("alto")
else =>
    echo("normal")
```

Todavia puedes usar `end`, pero ya no es obligatorio.

### Bucles

```boy
for i from 0 to 5 =>
    echo(i)
```

```boy
for item in ["a", "b", "c"] =>
    echo(item)
```

### Structs Y Classes

```boy
struct Vec2(x: number, y: number) =>
    task moved(dx: number, dy: number) -> Vec2 =>
        give Vec2(self.x + dx, self.y + dy)
```

## Sistema De Tipos

La capa de tipos actual no es un compilador estatico completo, pero ya sirve para subir el nivel del lenguaje:

- valida declaraciones
- valida argumentos al entrar a una tarea
- valida retornos
- valida campos en constructores
- valida listas tipadas como `list[T]`

Formas soportadas:

- `number`
- `text`
- `bool`
- `nil`
- `any`
- `list`
- `list[number]`
- `object`
- `task`
- `native-task`
- `job`
- nombres de tipos propios como `Player`, `Counter`, `Vec2`

## VM Y Runtime

### VM De Expresiones

BORY ahora compila expresiones a bytecode y las ejecuta en una VM de pila. El flujo de sentencias sigue usando el runtime actual, pero las expresiones dejan de depender solo de recursion AST.

Eso mejora la base tecnica para:

- ampliar cobertura de VM
- depuracion de bytecode
- optimizaciones futuras
- investigacion AOT/JIT

### Heap Y `gc`

El runtime ahora mantiene un registro del heap para listas y objetos. Desde el lenguaje puedes consultar ese estado:

```boy
var antes = gc.stats()
var despues = gc.collect()
echo(antes)
echo(despues)
```

`gc.collect()` hoy limpia el registro de tracking y reporta estado del heap administrado por el runtime. Es una base de gestion de memoria util, no un recolector final con compactacion.

## Modulos Estandar

- `math`
- `rand`
- `sys`
- `json`
- `text`
- `matrix`
- `clock`
- `net`
- `http`
- `flow`
- `gc`
- `screen`

## Paquetes Incluidos

### Bscreen

Envuelve al modulo nativo `screen`.

Permite:

- abrir ventanas
- limpiar framebuffers
- dibujar pixeles
- dibujar rectangulos
- presentar frames
- leer estado de ventana e input

Ejemplo:

```boy
use Bscreen as bs

var win = bs.open(320, 240, "Demo")
bs.clear(win, bs.rgb(15, 24, 40))
bs.rect(win, 16, 16, 80, 60, bs.rgb(90, 150, 255))
bs.present(win)
```

### Bgames

Es una capa mas comoda sobre `Bscreen`.

Incluye:

- dibujado de sprites por matrices
- ayuda para movimiento
- lectura de input
- deteccion simple de botones

Ejemplo:

```boy
use Bgames as game

var win = game.open(200, 140, "Input")
var state = game.input(win)
echo(state.keys)
```

### Bdata

Paquete para trabajo con datos y archivos.

Incluye:

- lectura y escritura de texto
- append
- lectura y escritura JSON
- lectura por lineas
- parseo CSV simple
- escritura CSV desde filas-objeto

Ejemplo:

```boy
use Bdata as data

data.write_json("save.json", {name: "bory", score: 99})
var save = data.read_json("save.json")
echo(save.score)
```

## Flujo CLI

```powershell
bory run .\main.boy
bory check .\main.boy
bory fmt .\main.boy
bory pkg list
```

## BORY Studio

El Studio ahora incluye:

- tamano inicial adaptado a la pantalla
- botones redondeados
- multiples terminales
- colores mas vivos
- busqueda integrada
- sugerencias de simbolos
- cierre automatico de parentesis, llaves y comillas

## Diagnosticos

Los errores ahora muestran:

- tipo de error
- codigo
- archivo
- linea y columna
- fragmento de codigo
- hint
- notas
- traza

Eso hace mucho mas legibles los fallos de tipos, las llamadas nativas y los errores dentro de tareas anidadas.
