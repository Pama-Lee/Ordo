package L3

default result := {"code": "DOMESTIC", "message": "domestic order", "discount_rate": 0.05, "final_amount": 0}

gold_base := 0.30  if { input.user.membership == "gold";   input.order.amount >= 10000 }
gold_base := 0.25  if { input.user.membership == "gold";   input.order.amount >= 5000; input.order.amount < 10000 }
gold_base := 0.20  if { input.user.membership == "gold";   input.order.amount >= 1000; input.order.amount < 5000 }
gold_base := 0.15  if { input.user.membership == "gold";   input.order.amount < 1000 }

silver_base := 0.20 if { input.user.membership == "silver"; input.order.amount >= 10000 }
silver_base := 0.15 if { input.user.membership == "silver"; input.order.amount >= 5000; input.order.amount < 10000 }
silver_base := 0.10 if { input.user.membership == "silver"; input.order.amount < 5000 }

bronze_base := 0.10 if { input.order.amount >= 10000 }
bronze_base := 0.05 if { input.order.amount < 10000 }

discount := gold_base   if { input.user.membership == "gold" }
discount := silver_base if { input.user.membership == "silver" }
discount := bronze_base if { not input.user.membership == "gold"; not input.user.membership == "silver" }

region_code := "INTL"     if { input.user.region == "international" }
region_code := "DOMESTIC" if { not input.user.region == "international" }

region_msg := "international order" if { input.user.region == "international" }
region_msg := "domestic order"      if { not input.user.region == "international" }

result := {
    "code": region_code,
    "message": region_msg,
    "discount_rate": discount,
    "final_amount": input.order.amount * (1 - discount),
}
