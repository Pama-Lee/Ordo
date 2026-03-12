package L4

default result := {"code": "PASS", "message": "approved", "risk_score": 0}

amount_tier := "critical" if { input.transaction.amount >= 100000 }
amount_tier := "high"     if { input.transaction.amount >= 50000;  input.transaction.amount < 100000 }
amount_tier := "medium"   if { input.transaction.amount >= 10000;  input.transaction.amount < 50000 }
amount_tier := "low"      if { input.transaction.amount >= 1000;   input.transaction.amount < 10000 }
amount_tier := "minimal"  if { input.transaction.amount < 1000 }

risk := input.user.profile.risk_score
txn_count := count(input.user.history.transactions)
trust := input.device.trust_level

# Stage 1+2: amount_tier + risk_score → base action
base_action := "block"   if { amount_tier == "critical"; risk >= 80 }
base_action := "review"  if { amount_tier == "critical"; risk >= 50; risk < 80 }
base_action := "flag"    if { amount_tier == "critical"; risk < 50 }
base_action := "review"  if { amount_tier == "high"; risk >= 80 }
base_action := "flag"    if { amount_tier == "high"; risk >= 50; risk < 80 }
base_action := "monitor" if { amount_tier == "high"; risk < 50 }
base_action := "flag"    if { amount_tier == "medium"; risk >= 80 }
base_action := "monitor" if { amount_tier == "medium"; risk < 80 }
base_action := "flag"    if { amount_tier == "low"; risk >= 90 }
base_action := "pass"    if { amount_tier == "low"; risk < 90 }
base_action := "flag"    if { amount_tier == "minimal"; risk >= 95 }
base_action := "pass"    if { amount_tier == "minimal"; risk < 95 }

# Stage 3: history check can escalate
hist_action := "block"   if { base_action == "block" }
hist_action := "review"  if { base_action == "review" }
hist_action := "flag"    if { base_action == "flag"; txn_count > 20 }
hist_action := "monitor" if { base_action == "flag"; txn_count <= 20 }
hist_action := "flag"    if { base_action == "monitor"; txn_count > 200 }
hist_action := "pass"    if { base_action == "monitor"; txn_count <= 200 }
hist_action := "monitor" if { base_action == "pass"; txn_count > 500 }
hist_action := "pass"    if { base_action == "pass"; txn_count <= 500 }

# Stage 4: device trust can de-escalate
final_action := "review"  if { hist_action == "block";   trust >= 8 }
final_action := "block"   if { hist_action == "block";   trust < 8 }
final_action := "flag"    if { hist_action == "review";  trust >= 5 }
final_action := "review"  if { hist_action == "review";  trust < 5 }
final_action := "monitor" if { hist_action == "flag";    trust >= 7 }
final_action := "flag"    if { hist_action == "flag";    trust < 7 }
final_action := "pass"    if { hist_action == "monitor"; trust >= 3 }
final_action := "monitor" if { hist_action == "monitor"; trust < 3 }
final_action := "monitor" if { hist_action == "pass";    trust < 2 }
final_action := "pass"    if { hist_action == "pass";    trust >= 2 }

# Risk score computation
computed_risk := risk * 0.4 + input.transaction.amount * 0.0001 * 0.3 + txn_count * 0.3 if { final_action == "review" }
computed_risk := risk * 0.5 + input.transaction.amount * 0.0001 * 0.3 + txn_count * 0.2 if { final_action == "flag" }
computed_risk := risk * 0.6 + input.transaction.amount * 0.0001 * 0.2 + txn_count * 0.2 if { final_action == "monitor" }
computed_risk := risk * 0.3 + input.transaction.amount * 0.0001 * 0.1 if { final_action == "pass" }
computed_risk := risk if { final_action == "block" }

codes := {"block": "BLOCK", "review": "REVIEW", "flag": "FLAG", "monitor": "MONITOR", "pass": "PASS"}
msgs := {"block": "transaction blocked", "review": "manual review required", "flag": "flagged for monitoring", "monitor": "enhanced monitoring", "pass": "approved"}

result := {
    "code": codes[final_action],
    "message": msgs[final_action],
    "risk_score": computed_risk,
}
