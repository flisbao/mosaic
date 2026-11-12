use crate::domain::language::Language;

pub fn unknown_field(language: Language) -> String {
    match language {
        Language::English => "unknown field",
        Language::BrazilianPortuguese => "campo desconhecido",
    }
    .to_string()
}

pub fn not_text_ignored(language: Language) -> String {
    match language {
        Language::English => "must be text; ignored",
        Language::BrazilianPortuguese => "precisa ser texto; ignorado",
    }
    .to_string()
}

pub fn not_a_list(language: Language) -> String {
    match language {
        Language::English => "must be a list",
        Language::BrazilianPortuguese => "precisa ser uma lista",
    }
    .to_string()
}

pub fn missing_field(language: Language) -> String {
    match language {
        Language::English => "required field missing",
        Language::BrazilianPortuguese => "campo obrigatório ausente",
    }
    .to_string()
}

pub fn unsupported_protocol(language: Language, version: u64) -> String {
    match language {
        Language::English => {
            format!("this widget requires protocol {version}; update mosaic")
        }
        Language::BrazilianPortuguese => {
            format!("este widget exige o protocolo {version}; atualize o mosaic")
        }
    }
}

pub fn root_not_an_object(language: Language) -> String {
    match language {
        Language::English => "the root must be an object",
        Language::BrazilianPortuguese => "a raiz precisa ser um objeto",
    }
    .to_string()
}

pub fn summary_required(language: Language) -> String {
    match language {
        Language::English => "summary: object required",
        Language::BrazilianPortuguese => "summary: objeto obrigatório",
    }
    .to_string()
}

pub fn summary_title_required(language: Language) -> String {
    match language {
        Language::English => "summary.title: text required",
        Language::BrazilianPortuguese => "summary.title: texto obrigatório",
    }
    .to_string()
}

pub fn text_cut(language: Language, limit: usize) -> String {
    match language {
        Language::English => format!("text cut at {limit} characters"),
        Language::BrazilianPortuguese => format!("texto cortado em {limit} caracteres"),
    }
}

pub fn not_text(language: Language) -> String {
    match language {
        Language::English => "must be text",
        Language::BrazilianPortuguese => "precisa ser texto",
    }
    .to_string()
}

pub fn unknown_flag(language: Language, name: &str) -> String {
    match language {
        Language::English => format!("unknown flag: {name}"),
        Language::BrazilianPortuguese => format!("flag desconhecida: {name}"),
    }
}

pub fn invalid_protocol_number(language: Language) -> String {
    match language {
        Language::English => "must be a positive integer",
        Language::BrazilianPortuguese => "precisa ser um número inteiro positivo",
    }
    .to_string()
}

pub fn not_an_object(language: Language) -> String {
    match language {
        Language::English => "must be an object",
        Language::BrazilianPortuguese => "precisa ser um objeto",
    }
    .to_string()
}

pub fn only_items_read(language: Language, limit: usize) -> String {
    match language {
        Language::English => format!("only {limit} items are read"),
        Language::BrazilianPortuguese => format!("apenas {limit} itens são lidos"),
    }
}

pub fn unknown_tone(language: Language, name: &str) -> String {
    match language {
        Language::English => format!("unknown tone: {name}"),
        Language::BrazilianPortuguese => format!("tom desconhecido: {name}"),
    }
}

pub fn block_not_an_object(language: Language) -> String {
    match language {
        Language::English => "block must be an object",
        Language::BrazilianPortuguese => "bloco precisa ser um objeto",
    }
    .to_string()
}

pub fn type_missing(language: Language) -> String {
    match language {
        Language::English => "type missing",
        Language::BrazilianPortuguese => "type ausente",
    }
    .to_string()
}

pub fn unknown_block_type(language: Language, name: &str) -> String {
    match language {
        Language::English => format!("unknown block type: {name}"),
        Language::BrazilianPortuguese => format!("tipo de bloco desconhecido: {name}"),
    }
}

pub fn needs_content(language: Language) -> String {
    match language {
        Language::English => "must have content",
        Language::BrazilianPortuguese => "precisa ter conteúdo",
    }
    .to_string()
}

pub fn invalid_item_id(language: Language, maximum: usize) -> String {
    match language {
        Language::English => format!(
            "id must be 1 to {maximum} characters, without control or formatting characters; ignored"
        ),
        Language::BrazilianPortuguese => format!(
            "id precisa ter de 1 a {maximum} caracteres, sem controle nem formatação; ignorado"
        ),
    }
}

pub fn not_an_object_ignored(language: Language) -> String {
    match language {
        Language::English => "must be an object; ignored",
        Language::BrazilianPortuguese => "precisa ser um objeto; ignorado",
    }
    .to_string()
}

pub fn reserved_key(language: Language) -> String {
    match language {
        Language::English => "reserved key; Mosaic sends the id of the selected item",
        Language::BrazilianPortuguese => "chave reservada; o Mosaic envia o id do item selecionado",
    }
    .to_string()
}

pub fn params_not_an_object(language: Language) -> String {
    match language {
        Language::English => "params must be an object; using {}",
        Language::BrazilianPortuguese => "params precisa ser um objeto; usando {}",
    }
    .to_string()
}

pub fn alert_not_an_object(language: Language) -> String {
    match language {
        Language::English => "must be an object; discarded",
        Language::BrazilianPortuguese => "precisa ser um objeto; descartado",
    }
    .to_string()
}

pub fn alert_title_missing(language: Language) -> String {
    match language {
        Language::English => "title missing; discarded",
        Language::BrazilianPortuguese => "título ausente; descartado",
    }
    .to_string()
}

pub fn invalid_alert_id(language: Language, maximum: usize) -> String {
    match language {
        Language::English => {
            format!("id must be text of 1 to {maximum} characters; alert discarded")
        }
        Language::BrazilianPortuguese => {
            format!("id precisa ser um texto de 1 a {maximum} caracteres; aviso descartado")
        }
    }
}

pub fn unknown_level(language: Language) -> String {
    match language {
        Language::English => "unknown level; using info",
        Language::BrazilianPortuguese => "nível desconhecido; usando info",
    }
    .to_string()
}

pub fn tags_not_a_list(language: Language) -> String {
    match language {
        Language::English => "must be a list; ignored",
        Language::BrazilianPortuguese => "precisa ser uma lista; ignorado",
    }
    .to_string()
}

pub fn tag_without_text(language: Language) -> String {
    match language {
        Language::English => "tag without text; ignored",
        Language::BrazilianPortuguese => "etiqueta sem texto; ignorada",
    }
    .to_string()
}

pub fn table_needs_a_column(language: Language) -> String {
    match language {
        Language::English => "must have at least one column",
        Language::BrazilianPortuguese => "precisa ter ao menos uma coluna",
    }
    .to_string()
}

pub fn unknown_alignment(language: Language) -> String {
    match language {
        Language::English => "unknown alignment; using left",
        Language::BrazilianPortuguese => "alinhamento desconhecido; usando left",
    }
    .to_string()
}

pub fn row_is_not_a_list_of_cells(language: Language) -> String {
    match language {
        Language::English => "must be a list of cells",
        Language::BrazilianPortuguese => "precisa ser uma lista de células",
    }
    .to_string()
}

pub fn row_width_mismatch(language: Language, cells: usize, columns: usize) -> String {
    match language {
        Language::English => format!("the row has {cells} cells and the table {columns} columns"),
        Language::BrazilianPortuguese => {
            format!("a linha tem {cells} células e a tabela {columns} colunas")
        }
    }
}

pub fn cell_not_text_or_object(language: Language) -> String {
    match language {
        Language::English => "cell must be text or an object",
        Language::BrazilianPortuguese => "célula precisa ser texto ou objeto",
    }
    .to_string()
}

pub fn protocol_error_prefix(language: Language) -> &'static str {
    match language {
        Language::English => "plugin output is not a valid snapshot JSON",
        Language::BrazilianPortuguese => "saída de plugin não é um JSON válido de snapshot",
    }
}

pub fn and_more_errors(language: Language, count: usize) -> String {
    match language {
        Language::English => format!("… and {count} more errors"),
        Language::BrazilianPortuguese => format!("… e mais {count} erros"),
    }
}

pub fn invalid_block_label(language: Language) -> &'static str {
    match language {
        Language::English => "invalid block",
        Language::BrazilianPortuguese => "bloco inválido",
    }
}

pub fn invalid_mini_label(language: Language) -> &'static str {
    match language {
        Language::English => "invalid mini",
        Language::BrazilianPortuguese => "mini inválido",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_assertions::assert_translated;

    type PlainMessage = fn(Language) -> String;

    #[test]
    fn every_protocol_message_is_translated() {
        let plain: [(&str, PlainMessage); 24] = [
            ("unknown field", unknown_field),
            ("not text ignored", not_text_ignored),
            ("not a list", not_a_list),
            ("missing", missing_field),
            ("root", root_not_an_object),
            ("summary", summary_required),
            ("title", summary_title_required),
            ("not text", not_text),
            ("number", invalid_protocol_number),
            ("object", not_an_object),
            ("block object", block_not_an_object),
            ("type", type_missing),
            ("content", needs_content),
            ("object ignored", not_an_object_ignored),
            ("reserved", reserved_key),
            ("params", params_not_an_object),
            ("alert object", alert_not_an_object),
            ("alert title", alert_title_missing),
            ("level", unknown_level),
            ("tags", tags_not_a_list),
            ("tag", tag_without_text),
            ("column", table_needs_a_column),
            ("alignment", unknown_alignment),
            ("row", row_is_not_a_list_of_cells),
        ];
        for (label, message) in plain {
            assert_translated(label, message);
        }
        assert_translated("cell", cell_not_text_or_object);
        assert_translated("version", |language| unsupported_protocol(language, 2));
        assert_translated("cut", |language| text_cut(language, 5));
        assert_translated("flag", |language| unknown_flag(language, "x"));
        assert_translated("items", |language| only_items_read(language, 5));
        assert_translated("tone", |language| unknown_tone(language, "x"));
        assert_translated("block type", |language| unknown_block_type(language, "x"));
        assert_translated("item id", |language| invalid_item_id(language, 5));
        assert_translated("alert id", |language| invalid_alert_id(language, 5));
        assert_translated("width", |language| row_width_mismatch(language, 1, 2));
        assert_translated("more", |language| and_more_errors(language, 2));
        assert_translated("prefix", |language| {
            protocol_error_prefix(language).to_string()
        });
        assert_translated("block label", |language| {
            invalid_block_label(language).to_string()
        });
        assert_translated("mini label", |language| {
            invalid_mini_label(language).to_string()
        });
    }
}
