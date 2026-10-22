# OpenSound

**OpenSound** es una aplicación libre y de código abierto para Linux. Permite controlar y configurar auriculares inalámbricos que usan el protocolo MDR v2, como los WH-1000XM5. Incluye una app GTK 4 / libadwaita y una extensión de GNOME Shell. No requiere cuentas, no tiene telemetría y no usa la nube.

> **OpenSound es un proyecto comunitario sin fines de lucro ni comerciales.** No existe una opción oficial para configurar estos auriculares desde Linux, así que OpenSound existe solo para ampliar su conectividad y permitir que sus dueños usen en su computador el hardware que ya tienen. No se vende, no tiene funciones de pago y **no está afiliado, respaldado, patrocinado ni aprobado por Sony Group Corporation** ni por ninguna de sus filiales. Consulta el [aviso legal](#aviso-legal).

[Read in English](README.md)

## Funciones

| Área | Ajustes |
|---|---|
| Estado en vivo | Batería y carga, códec activo (SBC / AAC / LDAC…), modelo y firmware |
| Control de sonido ambiental | Supresión de ruido / Sonido ambiente / Desactivado, nivel de ambiente (1–20), enfocar en la voz |
| Hablar para chatear | Activar o desactivar, sensibilidad de detección de voz (automática, alta, baja), tiempo hasta que finaliza el modo (unos 5 s, 15 s, 30 s o nunca) |
| Sonido | Ecualizador de 5 bandas (400, 1k, 2.5k, 6.3k y 16k Hz) de −10 a +10, y Clear Bass |
| Conexión | Conectar a 2 dispositivos a la vez (multipunto), calidad de conexión Bluetooth |
| Controles | Ajustes de operación [NC/AMB], panel de control con sensor táctil, pausa al quitarse los auriculares |
| Energía | Apagado automático, apagar desde la app o la extensión |
| Sistema | Versión de firmware, guía de voz (activar o desactivar; el idioma se muestra) |

Los cambios hechos en los auriculares, por ejemplo con los botones, se ven al instante en la interfaz. Algunos cambios reinician la conexión, como el del multipunto. Para ellos, los auriculares piden una confirmación y OpenSound la muestra como diálogo.

**Probado con:** WH-1000XM5, firmware 2.5.1. Otros modelos con el mismo protocolo pueden funcionar en parte.

## Requisitos

- Linux con BlueZ, sesión de usuario de systemd, GTK 4 y libadwaita ≥ 1.7. Sirven Fedora 42+, Debian 13, Ubuntu 25.04+ y Arch Linux.
- GNOME Shell 48–50 para la extensión de la barra superior. La app funciona en cualquier escritorio.
- Auriculares emparejados y conectados desde los ajustes de Bluetooth del sistema.

## Instalación

Descarga los archivos de la [última versión](https://github.com/CpuJP/OpenSound/releases/latest).

**Fedora (RPM):**

```sh
sudo dnf install ./opensound-*.x86_64.rpm
```

**Otras distribuciones (binarios precompilados, por usuario, sin root):**

```sh
tar xzf opensound-*-linux.tar.gz
cd opensound-*-linux
sh install.sh
```

**Desde el código fuente** (requiere Rust ≥ 1.92 y los paquetes de desarrollo de GTK 4 y libadwaita):

```sh
git clone https://github.com/CpuJP/OpenSound.git
cd OpenSound
sh install.sh        # compila con cargo e instala en ~/.local
```

Cualquiera de los métodos instala lo mismo:

- `opensound-daemon`: el servicio que mantiene la conexión con los auriculares. D-Bus lo inicia cuando hace falta y la unidad de usuario de systemd `opensound.service` lo mantiene activo.
- La app **OpenSound**, en el menú de aplicaciones o con el comando `opensound`.
- La extensión de GNOME Shell `opensound@cpujp.github.io`. Añade a la barra superior un botón con la batería que abre una tarjeta de control (batería, control de ruido, nivel de sonido ambiente y voz enfocada), y un botón en Ajustes rápidos.

En Wayland, cierra sesión y vuelve a entrar una vez. Después ejecuta `gnome-extensions enable opensound@cpujp.github.io`.

Para desinstalar, ejecuta `sudo dnf remove opensound` si usaste el RPM, o `sh uninstall.sh` en los demás casos.

## Arquitectura y protocolo

```
Auriculares ──RFCOMM (SPP)── daemon opensound ──D-Bus de sesión──┬── App GTK
                                                                 └── Extensión GNOME
```

Los auriculares aceptan una sola conexión de control. El daemon es el único proceso que la abre, y las interfaces son clientes D-Bus. El daemon y la app están escritos en Rust. La extensión está en JavaScript (GJS), porque GNOME Shell solo carga extensiones en ese lenguaje. La API y el protocolo están documentados en [README.md](README.md#architecture) y [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Limitaciones conocidas

- Por ahora, la interfaz solo está en español.
- No permite cambiar el idioma de la guía de voz, porque requiere descargar un paquete de firmware.
- No hay actualización de firmware.
- Al desactivar el panel táctil, los auriculares quitan el asistente de voz asignado. OpenSound no puede volver a asignarlo.

## Transparencia sobre IA

OpenSound se desarrolló con ayuda de un agente de programación con IA (Claude Code), bajo dirección y revisión humanas, y con pruebas en hardware real. Las instrucciones para agentes de IA que trabajen en el repositorio están en [AGENTS.md](AGENTS.md), y hay un índice para modelos de lenguaje en [llms.txt](llms.txt).

## Aviso legal

- OpenSound es un proyecto **no oficial**, independiente, comunitario y **sin fines de lucro** ni propósito comercial. **No** está afiliado, autorizado, mantenido, patrocinado ni respaldado por Sony Group Corporation, Sony Corporation, Sony Electronics Inc. ni ninguna de sus filiales.
- "Sony", "WH-1000XM5", "LDAC", "DSEE" y los nombres, marcas y logotipos relacionados son marcas comerciales o registradas de sus respectivos titulares. Aquí se usan solo para identificar el hardware compatible (uso nominativo) y no implican ninguna asociación.
- Este repositorio **no** contiene código, binarios, firmware, imágenes ni otros recursos propietarios de Sony ni de ningún otro fabricante. El conocimiento del protocolo se obtuvo con el único fin de lograr la interoperabilidad entre hardware adquirido legalmente por el autor y el sistema operativo Linux, para el que no existe software oficial.
- El software se ofrece **"tal cual", sin garantía de ningún tipo** (secciones 15–17 de la [licencia](LICENSE)). Su uso es bajo tu propia responsabilidad. Los autores no responden por daños en dispositivos, pérdida de datos, problemas de garantía ni cualquier otra consecuencia de su uso.

El texto completo está en [DISCLAIMER.md](DISCLAIMER.md).

## Licencia

Copyright © 2026 Juan Pablo Giraldo Collazos y colaboradores.

OpenSound es software libre: puedes redistribuirlo y modificarlo bajo los términos de la [Licencia Pública General de GNU, versión 3 o posterior](LICENSE).
