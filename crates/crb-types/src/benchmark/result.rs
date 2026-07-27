use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{
    benchmark::{golden::GoldenComment, judge::JudgeVerdict},
    finding::Finding,
};
#[cfg(feature = "seaorm-storage")]
use crate::{
    benchmark::{
        golden::{GoldenCommentColumn, GoldenCommentEntity, GoldenCommentModel},
        judge::{JudgeVerdictEntity, JudgeVerdictModel},
    },
    finding::{FindingActiveModel, FindingEntity, FindingModel},
};

/// Result of evaluating a benchmark PR.
///
/// Save is hand-written because of the tuple findings_with_verdicts field.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "pr_results", skip_save)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrResult {
    /// The [`crate::review::Review::id`] of this PR result.
    pub id: MagicTypeId,

    /// Optional FK back to the [`Benchmark`] that produced this result.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(nullable))]
    pub benchmark_id: Option<MagicTypeId>,

    /// Golden comments for this PR.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "GoldenCommentEntity")
    )]
    pub golden_comments: Vec<GoldenComment>,

    /// Findings and their corresponding verdicts.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "FindingEntity", tuple = "JudgeVerdictEntity")
    )]
    pub findings_with_verdicts: Vec<(Finding, JudgeVerdict)>,
}

#[cfg(feature = "seaorm-storage")]
impl crate::stor::Save for PrResult {
    async fn save(&self, db: &sea_orm::DatabaseConnection) -> Result<(), anyhow::Error> {
        use sea_orm::ActiveModelTrait;
        use sea_orm::IntoActiveModel;

        let active = PrResultActiveModel::from(self.clone());
        match active.clone().insert(db).await {
            Ok(_) => {}
            Err(e) if e.to_string().to_lowercase().contains("unique") => {
                active
                    .update(db)
                    .await
                    .map_err(|e| anyhow::anyhow!("update failed: {e}"))?;
            }
            Err(e) => return Err(anyhow::anyhow!("insert failed: {e}")),
        };

        for gc in &self.golden_comments {
            let mut cloned = gc.clone();
            cloned.pr_result_id = self.id.clone();
            cloned.save(db).await?;
        }

        for (finding, verdict) in &self.findings_with_verdicts {
            let finding_model = FindingModel {
                id: 0,
                pr_result_id: Some(self.id.to_string()),
                file: finding.file.clone(),
                line: finding.line,
                message: finding.message.clone(),
                severity: finding.severity,
                rule_code: finding.rule_code.clone(),
                severity_audited: finding.severity_audited,
                severity_audit_reason: finding.severity_audit_reason.clone(),
                evidence: finding.evidence.clone(),
                path_trace: finding.path_trace.clone(),
                confidence: finding.confidence.clone(),
                found_by: finding.found_by.clone(),
                agent_count: finding.agent_count,
                cross_validated: finding.cross_validated,
                cross_validated_by: finding.cross_validated_by,
                merged_from: finding.merged_from,
            };
            let saved_finding = finding_model.into_active_model().insert(db).await?;

            let verdict_model = JudgeVerdictModel {
                id: 0,
                finding_id: Some(saved_finding.id),
                linked_comment_id: None,
                reasoning: verdict.reasoning.clone(),
                match_: verdict.match_,
                confidence: verdict.confidence,
            };
            verdict_model.into_active_model().insert(db).await?;
        }

        Ok(())
    }
}
