# OpenSound

**OpenSound** es una aplicación libre y de código abierto para Linux. Permite controlar y configurar auriculares inalámbricos que usan el protocolo MDR v2, como los WH-1000XM5. Incluye una app GTK 4 / libadwaita y una extensión de GNOME Shell. No requiere cuentas, no tiene telemetría y no usa la nube.

> **OpenSound es un proyecto comunitario independiente. No está afiliado, respaldado, patrocinado ni aprobado por Sony Group Corporation ni por ninguna de sus filiales.** Consulta el [aviso legal](#aviso-legal).

[Read in English](README.md)

## Funciones

| Área | Ajustes |
|---|---|
| Estado en vivo | Batería y carga, códec activo (SBC / AAC / LDAC…), modelo y firmware |
| Control de sonido ambiental | Supresión de ruido / Sonido ambiente / Desactivado, nivel de ambiente (1–20), enfocar en la voz |
| Sonido | Ecualizador de 5 bandas (400, 1k, 2.5k, 6.3k y 16k Hz) de −10 a +10, y Clear Bass |
| Conexión | Conectar a 2 dispositivos a la vez (multipunto), calidad de conexión Bluetooth |
| Controles | Ajustes de operación [NC/AMB], panel de control con sensor táctil, pausa al quitarse los auriculares |
| Energía | Apagado automático |
| Sistema | Versión de firmware, guía de voz (activar o desactivar; el idioma se muestra) |

Los cambios hechos en los auriculares, por ejemplo con los botones, se ven al instante en la interfaz. Algunos cambios reinician la conexión, como el del multipunto. Para ellos, los auriculares piden una confirmación y OpenSound la muestra como diálogo.

**Probado con:** WH-1000XM5, firmware 2.5.1. Otros modelos con el mismo protocolo pueden funcionar en parte.

## Requisitos

- Linux con BlueZ, sesión de usuario de systemd y GNOME 48–50. Desarrollado en Fedora.
- Python ≥ 3.10, PyGObject, GTK 4 y libadwaita ≥ 1.7. Fedora Workstation los trae instalados.
- Auriculares emparejados y conectados desde los ajustes de Bluetooth del sistema.

## Instalación

```sh
git clone https://github.com/CpuJP/OpenSound.git
cd OpenSound
sh install.sh
```

El instalador es por usuario, no necesita root e instala todo en `~/.local`:

- `opensound.service`: servicio de usuario de systemd que mantiene la conexión con los auriculares.
- La app **OpenSound**, en el menú de aplicaciones o con el comando `opensound`.
- La extensión de GNOME Shell `opensound@cpujp.github.io`. Añade la batería a la barra superior y un botón en Ajustes rápidos.

En Wayland, cierra sesión y vuelve a entrar una vez. Después ejecuta `gnome-extensions enable opensound@cpujp.github.io`.

Para desinstalar todo, ejecuta `sh uninstall.sh`.

## Arquitectura y protocolo

```
Auriculares ──RFCOMM (SPP)── daemon opensound ──D-Bus de sesión──┬── App GTK
                                                                 └── Extensión GNOME
```

Los auriculares aceptan una sola conexión de control. El daemon es el único proceso que la abre, y las interfaces son clientes D-Bus. La API y el protocolo están documentados en [README.md](README.md#architecture) y [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Limitaciones conocidas

- Por ahora, la interfaz solo está en español.
- No permite cambiar el idioma de la guía de voz, porque requiere descargar un paquete de firmware.
- No hay actualización de firmware.
- Al desactivar el panel táctil, los auriculares quitan el asistente de voz asignado. Para recuperarlo hay que elegirlo de nuevo en la app oficial.

## Transparencia sobre IA

OpenSound se desarrolló con ayuda de un agente de programación con IA (Claude Code), bajo dirección y revisión humanas, y con pruebas en hardware real. Las instrucciones para agentes de IA que trabajen en el repositorio están en [AGENTS.md](AGENTS.md), y hay un índice para modelos de lenguaje en [llms.txt](llms.txt).

## Aviso legal

- OpenSound es un proyecto **no oficial**, independiente y desarrollado por la comunidad. **No** está afiliado, autorizado, mantenido, patrocinado ni respaldado por Sony Group Corporation, Sony Corporation, Sony Electronics Inc. ni ninguna de sus filiales.
- "Sony", "WH-1000XM5", "Sound Connect", "LDAC", "DSEE" y los nombres, marcas y logotipos relacionados son marcas comerciales o registradas de sus respectivos titulares. Aquí se usan solo para identificar el hardware compatible (uso nominativo) y no implican ninguna asociación.
- Este repositorio **no** contiene código, binarios, firmware, imágenes ni otros recursos propietarios de Sony ni de ningún otro fabricante. El conocimiento del protocolo se obtuvo con el único fin de lograr la interoperabilidad entre hardware adquirido legalmente por el autor y el sistema operativo Linux.
- El software se ofrece **"tal cual", sin garantía de ningún tipo** (secciones 15–17 de la [licencia](LICENSE)). Su uso es bajo tu propia responsabilidad. Los autores no responden por daños en dispositivos, pérdida de datos, problemas de garantía ni cualquier otra consecuencia de su uso.

El texto completo está en [DISCLAIMER.md](DISCLAIMER.md).

## Licencia

Copyright © 2026 Juan Pablo Giraldo Collazos y colaboradores.

OpenSound es software libre: puedes redistribuirlo y modificarlo bajo los términos de la [Licencia Pública General de GNU, versión 3 o posterior](LICENSE).
