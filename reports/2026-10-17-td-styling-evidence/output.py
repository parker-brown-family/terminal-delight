import sys,time
kind=sys.argv[1]
for i in range(45):print(f'row {i}: quiet text https://example.com/path code = 123',flush=True)
print('workload-ready',flush=True)
n=0
while True:
 if kind=='busy':
  print(f'\r\x1b[32mworking {n:08d}\x1b[0m',end='',flush=True);n+=1
 time.sleep(.05)
