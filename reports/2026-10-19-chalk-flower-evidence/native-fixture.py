import os,sys,time,signal
def draw(*_):
    print('\033[2J\033[H',end='')
    print('\033[35m  ❯\033[0m cargo check --workspace\n')
    print('  \033[32m✓\033[0m terminal-delight compiled successfully')
    print('  \033[32m✓\033[0m agent state updated')
    print('  \033[33m◇\033[0m git coordination · 2 overlapping files\n')
    print('  \033[35m│\033[0m # review required')
    print('  \033[35m│ modified:\033[0m src/ui/git_rail.rs')
    print('  \033[35m│ modified:\033[0m src/surface/workbench.rs\n')
    print('  \033[33mREVIEW\033[0m Changes ready for inspection\n')
    print('  \033[36m› Planning Git coordination redesign…\033[0m\n')
    print('  ───────────────────────────────────────\n')
    print('  \033[35m›\033[0m Ask anything…  \033[35m▌\033[0m',flush=True)
signal.signal(signal.SIGWINCH,draw)
draw()
tick=0
while True:
    if os.path.exists(os.environ.get('FLOWER_BUSY','/nonexistent')):
        rows=os.get_terminal_size(1).lines
        sys.stdout.write(f'\033[{rows};1H  frame {tick:06d}  ·  paper material study')
        sys.stdout.flush()
        tick+=1
    time.sleep(0.05)
