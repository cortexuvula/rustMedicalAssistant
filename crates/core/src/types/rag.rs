use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A retrieved chunk from the RAG system with its relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagResult {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub content: String,
    pub score: f32,
    pub source: SearchSource,
    pub metadata: RagChunkMetadata,
}

/// The retrieval strategy that produced a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Vector,
    Bm25,
    Graph,
    Fused,
}

/// Metadata attached to a retrieved chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunkMetadata {
    pub document_title: Option<String>,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub page_number: Option<u32>,
}

/// Configuration for a RAG search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub top_k: u32,
    pub similarity_threshold: f32,
    pub mmr_lambda: f32,
    pub enable_vector: bool,
    pub enable_bm25: bool,
    pub enable_graph: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            similarity_threshold: 0.75,
            mmr_lambda: 0.7,
            enable_vector: true,
            enable_bm25: true,
            enable_graph: true,
        }
    }
}

/// A query after expansion with synonyms or related terms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedQuery {
    pub original: String,
    pub expanded_terms: Vec<String>,
    pub full_query: String,
}

/// A chunk of a document prepared for indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub content: String,
    pub embedding: Vec<f32>,
    pub chunk_index: u32,
    pub metadata: RagChunkMetadata,
}

/// A node in the medical knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: Uuid,
    pub entity_type: EntityType,
    pub name: String,
    pub properties: serde_json::Value,
}

/// The type of a medical entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Drug,
    Condition,
    Procedure,
    Symptom,
    LabTest,
}

/// A directed relationship between two entities in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelation {
    pub from: Uuid,
    pub to: Uuid,
    pub relation_type: RelationType,
    pub properties: serde_json::Value,
}

/// The semantic type of a graph relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Treats,
    Contraindicates,
    Causes,
    Diagnoses,
    Indicates,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_config_defaults() {
        let config = SearchConfig::default();
        assert_eq!(config.top_k, 5);
        assert!((config.similarity_threshold - 0.75).abs() < f32::EPSILON);
        assert!((config.mmr_lambda - 0.7).abs() < f32::EPSILON);
        assert!(config.enable_vector);
        assert!(config.enable_bm25);
        assert!(config.enable_graph);
    }

    #[test]
    fn search_source_serializes() {
        let source = SearchSource::Fused;
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json, "fused");

        let vector: SearchSource = serde_json::from_str("\"vector\"").unwrap();
        assert_eq!(vector, SearchSource::Vector);
    }

    #[test]
    fn entity_type_serializes() {
        let et = EntityType::Drug;
        let json = serde_json::to_value(&et).unwrap();
        assert_eq!(json, "drug");

        let condition: EntityType = serde_json::from_str("\"condition\"").unwrap();
        assert_eq!(condition, EntityType::Condition);
    }

    #[test]
    fn relation_type_serializes() {
        let rt = RelationType::Treats;
        let json = serde_json::to_value(&rt).unwrap();
        assert_eq!(json, "treats");

        let contra: RelationType = serde_json::from_str("\"contraindicates\"").unwrap();
        assert_eq!(contra, RelationType::Contraindicates);
    }

    #[test]
    fn rag_result_round_trip() {
        let result = RagResult {
            chunk_id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            content: "Metformin treats type 2 diabetes.".into(),
            score: 0.92,
            source: SearchSource::Vector,
            metadata: RagChunkMetadata {
                document_title: Some("Drug Guide".into()),
                chunk_index: 0,
                total_chunks: 10,
                page_number: Some(1),
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: RagResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, result.content);
        assert_eq!(back.source, SearchSource::Vector);
    }
}
