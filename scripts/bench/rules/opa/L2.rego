package L2
default result := {"code": "FAIL", "message": "failed"}
result := {"code": "HIGH", "message": "high tier"} if { input.score >= 90 }
result := {"code": "MID",  "message": "mid tier"}  if { input.score >= 70; input.score < 90 }
result := {"code": "LOW",  "message": "low tier"}  if { input.score >= 50; input.score < 70 }
