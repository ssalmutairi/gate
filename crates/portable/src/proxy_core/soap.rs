use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use serde_json::{Map, Value};
use std::io::Cursor;

pub fn json_to_soap_xml(
    json_body: &Value,
    input_element: &str,
    target_namespace: &str,
) -> Result<Vec<u8>, String> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new("1.0", Some("utf-8"), None)))
        .map_err(|e| format!("XML write error: {}", e))?;

    let mut envelope = BytesStart::new("soap:Envelope");
    envelope.push_attribute(("xmlns:soap", "http://schemas.xmlsoap.org/soap/envelope/"));
    envelope.push_attribute(("xmlns:tns", target_namespace));
    writer
        .write_event(Event::Start(envelope))
        .map_err(|e| format!("XML write error: {}", e))?;

    writer
        .write_event(Event::Start(BytesStart::new("soap:Body")))
        .map_err(|e| format!("XML write error: {}", e))?;

    let element_tag = format!("tns:{}", input_element);
    writer
        .write_event(Event::Start(BytesStart::new(&element_tag)))
        .map_err(|e| format!("XML write error: {}", e))?;

    if let Some(obj) = json_body.as_object() {
        write_json_fields_as_xml(&mut writer, obj, "tns")?;
    }

    writer
        .write_event(Event::End(BytesEnd::new(&element_tag)))
        .map_err(|e| format!("XML write error: {}", e))?;

    writer
        .write_event(Event::End(BytesEnd::new("soap:Body")))
        .map_err(|e| format!("XML write error: {}", e))?;

    writer
        .write_event(Event::End(BytesEnd::new("soap:Envelope")))
        .map_err(|e| format!("XML write error: {}", e))?;

    Ok(writer.into_inner().into_inner())
}

fn write_json_fields_as_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    obj: &Map<String, Value>,
    ns_prefix: &str,
) -> Result<(), String> {
    for (key, value) in obj {
        let field_tag = format!("{}:{}", ns_prefix, key);
        writer
            .write_event(Event::Start(BytesStart::new(&field_tag)))
            .map_err(|e| format!("XML write error: {}", e))?;

        match value {
            Value::Object(nested) => {
                write_json_fields_as_xml(writer, nested, ns_prefix)?;
            }
            Value::Array(arr) => {
                for item in arr {
                    if let Value::Object(item_obj) = item {
                        write_json_fields_as_xml(writer, item_obj, ns_prefix)?;
                    } else {
                        let text = value_to_text(item);
                        writer
                            .write_event(Event::Text(BytesText::new(&text)))
                            .map_err(|e| format!("XML write error: {}", e))?;
                    }
                }
            }
            _ => {
                let text = value_to_text(value);
                writer
                    .write_event(Event::Text(BytesText::new(&text)))
                    .map_err(|e| format!("XML write error: {}", e))?;
            }
        }

        writer
            .write_event(Event::End(BytesEnd::new(&field_tag)))
            .map_err(|e| format!("XML write error: {}", e))?;
    }
    Ok(())
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

pub fn soap_xml_to_json(
    xml_body: &[u8],
    output_element: &str,
) -> Result<Value, String> {
    let xml_str = std::str::from_utf8(xml_body)
        .map_err(|e| format!("Invalid UTF-8 in SOAP response: {}", e))?;

    let mut reader = Reader::from_str(xml_str);
    let mut buf = Vec::new();

    let mut in_body = false;
    let mut in_output_element = false;
    let mut current_field: Option<String> = None;
    let mut result = Map::new();
    let mut depth_in_output = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag);

                if local == "Body" {
                    in_body = true;
                } else if in_body && local == output_element {
                    in_output_element = true;
                    depth_in_output = 0;
                } else if in_output_element {
                    if depth_in_output == 0 {
                        current_field = Some(local.to_string());
                    }
                    depth_in_output += 1;
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_output_element {
                    if let Some(ref field) = current_field {
                        let text = e.unescape().unwrap_or_default().to_string();
                        let value = if let Ok(n) = text.parse::<i64>() {
                            Value::Number(n.into())
                        } else if let Ok(n) = text.parse::<f64>() {
                            Value::Number(serde_json::Number::from_f64(n).unwrap_or(0.into()))
                        } else if text == "true" {
                            Value::Bool(true)
                        } else if text == "false" {
                            Value::Bool(false)
                        } else {
                            Value::String(text)
                        };
                        result.insert(field.clone(), value);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag);

                if local == "Body" {
                    in_body = false;
                } else if in_output_element && local == output_element {
                    in_output_element = false;
                } else if in_output_element {
                    depth_in_output -= 1;
                    if depth_in_output == 0 {
                        current_field = None;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(Value::Object(result))
}

fn strip_ns(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn extract_url_path(url: &str) -> String {
    url.find("://")
        .and_then(|i| url[i + 3..].find('/').map(|j| &url[i + 3 + j..]))
        .unwrap_or("/")
        .to_string()
}

pub const MAX_SOAP_BODY_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SoapOperationMeta {
    pub soap_action: String,
    pub operation_name: String,
    pub input_element: String,
    pub output_element: String,
    pub target_namespace: String,
    pub soap_endpoint: String,
    pub endpoint_path: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SoapServiceMeta {
    pub target_namespace: String,
    pub soap_endpoint: String,
    pub operations: std::collections::HashMap<String, SoapOperationMeta>,
}

impl SoapServiceMeta {
    pub fn from_json(val: &Value) -> Option<Self> {
        let obj = val.as_object()?;
        let target_namespace = obj.get("target_namespace")?.as_str()?.to_string();
        let soap_endpoint = obj.get("soap_endpoint")?.as_str()?.to_string();
        let ops_obj = obj.get("operations")?.as_object()?;

        let mut operations = std::collections::HashMap::new();
        for (path, op_val) in ops_obj {
            let op = op_val.as_object()?;
            operations.insert(
                path.clone(),
                SoapOperationMeta {
                    soap_action: op.get("soap_action")?.as_str()?.to_string(),
                    operation_name: op.get("operation_name")?.as_str()?.to_string(),
                    input_element: op.get("input_element")?.as_str()?.to_string(),
                    output_element: op.get("output_element")?.as_str()?.to_string(),
                    target_namespace: target_namespace.clone(),
                    endpoint_path: extract_url_path(&soap_endpoint),
                    soap_endpoint: soap_endpoint.clone(),
                },
            );
        }

        Some(SoapServiceMeta {
            target_namespace,
            soap_endpoint,
            operations,
        })
    }
}
