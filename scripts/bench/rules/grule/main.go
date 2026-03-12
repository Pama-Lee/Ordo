package main

import (
	"encoding/json"
	"fmt"
	"net/http"

	"github.com/hyperjumptech/grule-rule-engine/ast"
	"github.com/hyperjumptech/grule-rule-engine/builder"
	"github.com/hyperjumptech/grule-rule-engine/engine"
	"github.com/hyperjumptech/grule-rule-engine/pkg"
)

// ─── Shared types ───

type L1Input struct{ Score float64 }
type L1Output struct{ Code, Message string }

type L2Input struct{ Score float64 }
type L2Output struct{ Code, Message string }

type L3Input struct {
	Membership string
	Amount     float64
	Region     string
}
type L3Output struct {
	Code         string
	Message      string
	DiscountRate float64
	FinalAmount  float64
}

type L4Input struct {
	TxnAmount  float64
	RiskScore  float64
	TxnCount   int64
	TrustLevel float64
}
type L4Output struct {
	Code      string
	Message   string
	RiskScore float64
}

// ─── GRL Rules ───

const grlL1 = `
rule Pass "Pass" salience 2 { when Input.Score >= 60 then Output.Code = "PASS"; Output.Message = "passed"; Retract("Pass"); }
rule Fail "Fail" salience 1 { when Input.Score < 60 then Output.Code = "FAIL"; Output.Message = "failed"; Retract("Fail"); }
`

const grlL2 = `
rule High "High" salience 4 { when Input.Score >= 90 then Output.Code = "HIGH"; Output.Message = "high tier"; Retract("High"); }
rule Mid "Mid" salience 3 { when Input.Score >= 70 && Input.Score < 90 then Output.Code = "MID"; Output.Message = "mid tier"; Retract("Mid"); }
rule Low "Low" salience 2 { when Input.Score >= 50 && Input.Score < 70 then Output.Code = "LOW"; Output.Message = "low tier"; Retract("Low"); }
rule Fail "Fail" salience 1 { when Input.Score < 50 then Output.Code = "FAIL"; Output.Message = "failed"; Retract("Fail"); }
`

const grlL3 = `
rule GoldHigh "GH" salience 10 { when Input.Membership == "gold" && Input.Amount >= 10000 then Output.DiscountRate = 0.30; Output.FinalAmount = Input.Amount * 0.70; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("GH"); }
rule GoldMedH "GMH" salience 9 { when Input.Membership == "gold" && Input.Amount >= 5000 && Input.Amount < 10000 then Output.DiscountRate = 0.25; Output.FinalAmount = Input.Amount * 0.75; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("GMH"); }
rule GoldMedL "GML" salience 8 { when Input.Membership == "gold" && Input.Amount >= 1000 && Input.Amount < 5000 then Output.DiscountRate = 0.20; Output.FinalAmount = Input.Amount * 0.80; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("GML"); }
rule GoldLow "GL" salience 7 { when Input.Membership == "gold" && Input.Amount < 1000 then Output.DiscountRate = 0.15; Output.FinalAmount = Input.Amount * 0.85; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("GL"); }
rule SilverHigh "SH" salience 6 { when Input.Membership == "silver" && Input.Amount >= 10000 then Output.DiscountRate = 0.20; Output.FinalAmount = Input.Amount * 0.80; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("SH"); }
rule SilverMed "SM" salience 5 { when Input.Membership == "silver" && Input.Amount >= 5000 && Input.Amount < 10000 then Output.DiscountRate = 0.15; Output.FinalAmount = Input.Amount * 0.85; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("SM"); }
rule SilverLow "SL" salience 4 { when Input.Membership == "silver" && Input.Amount < 5000 then Output.DiscountRate = 0.10; Output.FinalAmount = Input.Amount * 0.90; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("SL"); }
rule BronzeHigh "BH" salience 3 { when Input.Amount >= 10000 then Output.DiscountRate = 0.10; Output.FinalAmount = Input.Amount * 0.90; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("BH"); }
rule BronzeLow "BL" salience 2 { when Input.Amount < 10000 then Output.DiscountRate = 0.05; Output.FinalAmount = Input.Amount * 0.95; Output.Code = "DOMESTIC"; Output.Message = "domestic order"; Retract("BL"); }
`

const grlL4 = `
rule CritHighBlock "CHB" salience 20 { when Input.TxnAmount >= 100000 && Input.RiskScore >= 80 && Input.TrustLevel < 8 then Output.Code = "BLOCK"; Output.Message = "transaction blocked"; Output.RiskScore = Input.RiskScore; Retract("CHB"); }
rule CritHighReview "CHR" salience 19 { when Input.TxnAmount >= 100000 && Input.RiskScore >= 80 && Input.TrustLevel >= 8 then Output.Code = "REVIEW"; Output.Message = "manual review required"; Output.RiskScore = Input.RiskScore * 0.4 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.3; Retract("CHR"); }
rule CritMedReview "CMR" salience 18 { when Input.TxnAmount >= 100000 && Input.RiskScore >= 50 && Input.RiskScore < 80 then Output.Code = "REVIEW"; Output.Message = "manual review required"; Output.RiskScore = Input.RiskScore * 0.4 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.3; Retract("CMR"); }
rule CritLowFlag "CLF" salience 17 { when Input.TxnAmount >= 100000 && Input.RiskScore < 50 then Output.Code = "FLAG"; Output.Message = "flagged for monitoring"; Output.RiskScore = Input.RiskScore * 0.5 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.2; Retract("CLF"); }
rule HighRiskReview "HRR" salience 16 { when Input.TxnAmount >= 50000 && Input.TxnAmount < 100000 && Input.RiskScore >= 80 then Output.Code = "REVIEW"; Output.Message = "manual review required"; Output.RiskScore = Input.RiskScore * 0.4 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.3; Retract("HRR"); }
rule HighMedFlag "HMF" salience 15 { when Input.TxnAmount >= 50000 && Input.TxnAmount < 100000 && Input.RiskScore >= 50 && Input.RiskScore < 80 then Output.Code = "FLAG"; Output.Message = "flagged for monitoring"; Output.RiskScore = Input.RiskScore * 0.5 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.2; Retract("HMF"); }
rule HighLowMonitor "HLM" salience 14 { when Input.TxnAmount >= 50000 && Input.TxnAmount < 100000 && Input.RiskScore < 50 then Output.Code = "MONITOR"; Output.Message = "enhanced monitoring"; Output.RiskScore = Input.RiskScore * 0.6 + Input.TxnAmount * 0.0001 * 0.2 + Input.TxnCount * 0.2; Retract("HLM"); }
rule MedHighFlag "MHF" salience 13 { when Input.TxnAmount >= 10000 && Input.TxnAmount < 50000 && Input.RiskScore >= 80 then Output.Code = "FLAG"; Output.Message = "flagged for monitoring"; Output.RiskScore = Input.RiskScore * 0.5 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.2; Retract("MHF"); }
rule MedLowMonitor "MLM" salience 12 { when Input.TxnAmount >= 10000 && Input.TxnAmount < 50000 && Input.RiskScore < 80 then Output.Code = "MONITOR"; Output.Message = "enhanced monitoring"; Output.RiskScore = Input.RiskScore * 0.6 + Input.TxnAmount * 0.0001 * 0.2 + Input.TxnCount * 0.2; Retract("MLM"); }
rule LowHighFlag "LHF" salience 11 { when Input.TxnAmount >= 1000 && Input.TxnAmount < 10000 && Input.RiskScore >= 90 then Output.Code = "FLAG"; Output.Message = "flagged for monitoring"; Output.RiskScore = Input.RiskScore * 0.5 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.2; Retract("LHF"); }
rule LowPass "LP" salience 10 { when Input.TxnAmount >= 1000 && Input.TxnAmount < 10000 && Input.RiskScore < 90 then Output.Code = "PASS"; Output.Message = "approved"; Output.RiskScore = Input.RiskScore * 0.3 + Input.TxnAmount * 0.0001 * 0.1; Retract("LP"); }
rule MinHighFlag "MiHF" salience 9 { when Input.TxnAmount < 1000 && Input.RiskScore >= 95 then Output.Code = "FLAG"; Output.Message = "flagged for monitoring"; Output.RiskScore = Input.RiskScore * 0.5 + Input.TxnAmount * 0.0001 * 0.3 + Input.TxnCount * 0.2; Retract("MiHF"); }
rule MinPass "MiP" salience 1 { when Input.TxnAmount < 1000 then Output.Code = "PASS"; Output.Message = "approved"; Output.RiskScore = Input.RiskScore * 0.3 + Input.TxnAmount * 0.0001 * 0.1; Retract("MiP"); }
`

var libs map[string]*ast.KnowledgeLibrary

func init() {
	libs = make(map[string]*ast.KnowledgeLibrary)
	rules := map[string]string{"L1": grlL1, "L2": grlL2, "L3": grlL3, "L4": grlL4}
	for name, grl := range rules {
		lib := ast.NewKnowledgeLibrary()
		rb := builder.NewRuleBuilder(lib)
		if err := rb.BuildRuleFromResource(name, "0.1.0", pkg.NewBytesResource([]byte(grl))); err != nil {
			panic(fmt.Sprintf("Failed to build %s: %v", name, err))
		}
		libs[name] = lib
	}
}

func handleL1(w http.ResponseWriter, r *http.Request) {
	var req struct{ Score float64 `json:"score"` }
	json.NewDecoder(r.Body).Decode(&req)
	input := &L1Input{Score: req.Score}
	output := &L1Output{}
	kb, _ := libs["L1"].NewKnowledgeBaseInstance("L1", "0.1.0")
	dc := ast.NewDataContext()
	dc.Add("Input", input)
	dc.Add("Output", output)
	eng := &engine.GruleEngine{MaxCycle: 10}
	eng.Execute(dc, kb)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"code": output.Code, "message": output.Message})
}

func handleL2(w http.ResponseWriter, r *http.Request) {
	var req struct{ Score float64 `json:"score"` }
	json.NewDecoder(r.Body).Decode(&req)
	input := &L2Input{Score: req.Score}
	output := &L2Output{}
	kb, _ := libs["L2"].NewKnowledgeBaseInstance("L2", "0.1.0")
	dc := ast.NewDataContext()
	dc.Add("Input", input)
	dc.Add("Output", output)
	eng := &engine.GruleEngine{MaxCycle: 10}
	eng.Execute(dc, kb)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"code": output.Code, "message": output.Message})
}

func handleL3(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Membership string  `json:"membership"`
		Amount     float64 `json:"amount"`
		Region     string  `json:"region"`
	}
	json.NewDecoder(r.Body).Decode(&req)
	input := &L3Input{Membership: req.Membership, Amount: req.Amount, Region: req.Region}
	output := &L3Output{}
	kb, _ := libs["L3"].NewKnowledgeBaseInstance("L3", "0.1.0")
	dc := ast.NewDataContext()
	dc.Add("Input", input)
	dc.Add("Output", output)
	eng := &engine.GruleEngine{MaxCycle: 20}
	eng.Execute(dc, kb)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"code": output.Code, "message": output.Message,
		"discount_rate": output.DiscountRate, "final_amount": output.FinalAmount,
	})
}

func handleL4(w http.ResponseWriter, r *http.Request) {
	var req struct {
		TxnAmount  float64 `json:"txn_amount"`
		RiskScore  float64 `json:"risk_score"`
		TxnCount   int64   `json:"txn_count"`
		TrustLevel float64 `json:"trust_level"`
	}
	json.NewDecoder(r.Body).Decode(&req)
	input := &L4Input{TxnAmount: req.TxnAmount, RiskScore: req.RiskScore, TxnCount: req.TxnCount, TrustLevel: req.TrustLevel}
	output := &L4Output{}
	kb, _ := libs["L4"].NewKnowledgeBaseInstance("L4", "0.1.0")
	dc := ast.NewDataContext()
	dc.Add("Input", input)
	dc.Add("Output", output)
	eng := &engine.GruleEngine{MaxCycle: 30}
	eng.Execute(dc, kb)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"code": output.Code, "message": output.Message, "risk_score": output.RiskScore,
	})
}

func main() {
	http.HandleFunc("/execute/L1", handleL1)
	http.HandleFunc("/execute/L2", handleL2)
	http.HandleFunc("/execute/L3", handleL3)
	http.HandleFunc("/execute/L4", handleL4)
	http.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) { w.Write([]byte(`{"status":"ok"}`)) })
	fmt.Println("grule on :8080")
	http.ListenAndServe(":8080", nil)
}
