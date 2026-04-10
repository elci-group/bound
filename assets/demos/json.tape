Output assets/demos/json.gif

Set FontSize 14
Set Width 900
Set Height 500
Set Theme "Dracula"

Type "bound [.rs] -- --json --out output.json src"
Sleep 500ms
Enter
Sleep 1s

Type "cat output.json | head -30"
Sleep 500ms
Enter
Sleep 2s
