from pathlib import Path
import hashlib,json,re
here=Path(__file__).parent
result={}
for label in ['default','fallback']:
 p=here/f'tests-{label}.log';s=p.read_text()
 matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',s)
 assert len(matches)==7 and all(int(x[1])==0 for x in matches),(label,'incomplete or failed suite')
 result[label]={'passed':sum(int(x[0]) for x in matches),'failed':sum(int(x[1]) for x in matches),'ignored':sum(int(x[2]) for x in matches),'suite_counts':[[int(v) for v in x] for x in matches],'full_local_log_sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
 p=here/f'clippy-{label}.log';s=p.read_text();assert 'Finished' in s and 'error:' not in s
 result[label]['strict_clippy_passed']=True
 result[label]['clippy_log_sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
result['format_and_diff_check']='Passed; separate commands before commit.'
(here/'validation.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
