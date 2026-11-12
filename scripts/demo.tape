# Grava o GIF da demonstração com o VHS (https://github.com/charmbracelet/vhs):
#   vhs scripts/demo.tape
# Passo manual: depois de gravar, publique demo.gif e ponha a imagem no README.
Output demo.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 12
Set TypingSpeed 60ms

Env MOSAIC_LANGUAGE "en"

Type "scripts/demo.sh"
Enter
Sleep 8s

Type "?"
Sleep 3s
Escape
Sleep 500ms

Tab
Sleep 2s
Tab
Sleep 2s
Shift+Tab
Sleep 1s

Type "+"
Sleep 3s
Escape
Sleep 500ms

Type "q"
Sleep 1s
