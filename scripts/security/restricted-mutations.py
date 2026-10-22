#!/usr/bin/env python3
"""Actual-module mutation checks; no changes to the source repository."""
import argparse,hashlib,json,os,pathlib,shutil,subprocess
from receipt import source as source_receipt
p=argparse.ArgumentParser();p.add_argument('--repo',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args()
root=a.out.resolve();root.mkdir(parents=True,exist_ok=False)
(root/'src/restricted').mkdir(parents=True);(root/'tests').mkdir()
(root/'Cargo.toml').write_text('[package]\nname="terminal-delight"\nversion="0.0.0"\nedition="2021"\n[dependencies]\nserde={version="1.0.228",features=["derive"]}\nserde_json="1.0"\nlibc="0.2.186"\n')
(root/'src/main.rs').write_text('''#![allow(dead_code)]
mod restricted;
mod ctl { pub(crate) fn restricted_note_target()->Result<crate::restricted::Note,String>{Err("No hosted pane in isolated module harness".into())} }
fn main(){let a:Vec<String>=std::env::args().collect();if a.get(1).map(String::as_str)!=Some("restricted"){std::process::exit(2)}std::process::exit(restricted::run_cli(&a[2..]));}
''')
recorded_source=source_receipt(a.repo)
source=(a.repo/'app/src/restricted.rs').read_text();filter_source=(a.repo/'app/src/restricted/seccomp.rs').read_text()
shutil.copyfile(a.repo/'app/tests/restricted.rs',root/'tests/restricted.rs')
shutil.copyfile(a.repo/'app/src/restricted/work.rs',root/'src/restricted/work.rs')
mod=root/'src/restricted.rs';filter_path=root/'src/restricted/seccomp.rs';work_path=root/'src/restricted/work.rs';work_source=work_path.read_text()
def restore():mod.write_text(source);filter_path.write_text(filter_source);work_path.write_text(work_source)
restore()
env=dict(os.environ,CARGO_TARGET_DIR=str(root/'target'),CARGO_BUILD_JOBS='1')
def run(label,args):
    r=subprocess.run(['cargo','test','--offline',*args],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=180)
    text=r.stdout.decode(errors='replace');(root/(label+'.log')).write_text(text);return r.returncode,text
for label,args in [('baseline-unit',['--bin','terminal-delight','--','--test-threads=1']),('baseline-os',['--test','restricted','--','--ignored','--test-threads=1'])]:
    code,text=run(label,args);assert code==0,text
cases=[
('hidden-root-policy','work','!name.as_bytes().starts_with(b".")','true','home_descendants_are_explicit_grants_but_hidden_and_external_roots_are_refused',False),
 ('mount-identity','work','id.mount != root.mount || ','','mount_identity_compares_mount_id_even_for_the_same_device',False),
 ('unknown-target-field','module','deny_unknown_fields','', 'a_text_only_gateway_cannot_select_a_target_grant_or_delegate',False),
('ungranted-note','module','if self.revoked || !self.note','if self.revoked','access_requests_are_bounded_proposals_and_never_change_grants',False),
('revoked-note','module','if self.revoked || !self.note','if !self.note','a_text_only_gateway_cannot_select_a_target_grant_or_delegate',False),
('self-revoke','module','Request::Revoke if operator','Request::Revoke','a_text_only_gateway_cannot_select_a_target_grant_or_delegate',False),
('terminal-control-output','module',"!c.is_control() || matches!(c, '\\n' | '\\r' | '\\t')",'true','output_cannot_invoke_terminal_protocol_even_when_utf8_is_split',False),
('connect-filter','filter','libc::SYS_connect,','', 'a_later_host_socket_in_the_work_tree_cannot_bypass_the_gateway',True),
('addressed-send','filter','libc::SYS_sendto as u32','libc::SYS_getpid as u32','no_new_connection_or_addressed_send_can_bypass_the_preconnected_channel',False),
('kernel-interfaces','filter','libc::SYS_keyctl,','libc::SYS_getpid,', 'kernel_interfaces_no_offline_build_needs_are_refused',False),
 ('inherited-descriptor','module','libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32)','0','offline_work_completes_without_host_files_services_processes_credentials_or_control',True),
]
results=[]
for label,which,before,after,test,os_probe in cases:
    restore();path={'module':mod,'filter':filter_path,'work':work_path}[which];s=path.read_text();assert before in s,(label,before)
    # Removing a serde attribute argument must not leave an invalid comma.
    if label=='unknown-target-field':s=s.replace(', deny_unknown_fields','')
    else:s=s.replace(before,after,1)
    path.write_text(s)
    args=['--test','restricted',test,'--','--ignored','--test-threads=1'] if os_probe else ['--bin','terminal-delight',test,'--','--test-threads=1']
    code,text=run(label,args)
    killed=code!=0 and 'test result: FAILED.' in text and 'could not compile' not in text
    results.append({'mutation':label,'test':test,'result':'killed' if killed else 'survived_or_invalid','exit':code})
    print(json.dumps(results[-1]),flush=True)
    assert killed,text
restore()
assert source_receipt(a.repo)['source_fingerprint']==recorded_source['source_fingerprint']
receipt={'method':'Actual module and seccomp source in a small harness. Host-pane target is unavailable here and tested separately through the real native window.','source_sha256':hashlib.sha256(source.encode()).hexdigest(),'filter_sha256':hashlib.sha256(filter_source.encode()).hexdigest(),'baseline_unit':'pass','baseline_os':'5 passed','source':recorded_source,'mutations':results}
(root/'receipt.json').write_text(json.dumps(receipt,indent=2))
print(root/'receipt.json')
