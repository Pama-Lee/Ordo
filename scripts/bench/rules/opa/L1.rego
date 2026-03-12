package L1
default result := {"code": "FAIL", "message": "failed"}
result := {"code": "PASS", "message": "passed"} if { input.score >= 60 }
