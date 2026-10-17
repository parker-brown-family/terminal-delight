from pathlib import Path
import hashlib,json,subprocess,platform,os
root=Path(__file__).resolve().parents[2]
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
files=['app/src/pane.rs','app/src/pane/styling.rs','app/src/pane/doc.rs','app/src/docview.rs','app/src/theme.rs','app/Cargo.lock']
r={'source_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_sha256':{p:sha(root/p) for p in files},'baseline_binary':{'path':'/tmp/td-efficiency-baseline-a5fab29','sha256':sha('/tmp/td-efficiency-baseline-a5fab29')},'candidate_binary':{'path':'/tmp/td-efficiency-candidate-production','sha256':sha('/tmp/td-efficiency-candidate-production')},'build':'cargo build --release --locked (both binaries, without GPUI test-support)','rust':subprocess.check_output(['rustc','--version'],text=True).strip(),'kernel':platform.release(),'logical_cpus':os.cpu_count(),'grid_cell_dimensions':None,'grid_cell_dimensions_reason':'Not recorded by this runner; equal window geometry and pane layout are retained.','input_latency_ms':None,'input_latency_reason':'No end-to-end input/presentation instrument in this run.','output_target_hz':20,'actual_output_rate_hz':None,'actual_output_rate_reason':'Script readiness is checked, but actual writes per second are not counted.','installed':False}
(root/'reports/2026-10-17-td-styling-evidence/provenance.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r,indent=2))
