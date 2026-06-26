use super::*;
use super::constants::{GROUP_LABEL_HEIGHT, NODE_GAP};
use crate::layout::constants;
use crate::layout::NodeLayout;
use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Group, Identifier, Relation,
        SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;

    fn entity(id: &str, label: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span: Span::dummy(),
        }
    }

    fn entity_in_group(id: &str, label: &str, group: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked(group)),
            span: Span::dummy(),
        }
    }

    fn relation(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_group(id: &str, label: &str, entity_ids: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: entity_ids.into_iter().map(|e| Identifier::new_unchecked(e)).collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    fn diagram(entities: Vec<Entity>, relations: Vec<Relation>, groups: Vec<Group>) -> Diagram {
        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities,
            relations,
            groups,
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        }
    }

    #[test]
    fn arch_v2_layout_simple_chain() {
        let d = diagram(
            vec![entity("a", "API"), entity("b", "Service"), entity("c", "DB")],
            vec![relation("a", "b"), relation("b", "c")],
            vec![],
        );
        let result = ArchitectureV2Layout::default().compute(&d);
        assert_eq!(result.nodes.len(), 3);
        // a 应该在 b 上方，b 应该在 c 上方
        let a_y = result.nodes["a"].y;
        let b_y = result.nodes["b"].y;
        let c_y = result.nodes["c"].y;
        assert!(a_y < b_y, "a should be above b");
        assert!(b_y < c_y, "b should be above c");
    }

    #[test]
    fn arch_v2_layout_with_groups() {
        let g1 = make_group("frontend", "Frontend", vec!["fe1", "fe2"]);
        let g2 = make_group("backend", "Backend", vec!["be1", "be2"]);

        let d = diagram(
            vec![
                entity_in_group("fe1", "FE1", "frontend"),
                entity_in_group("fe2", "FE2", "frontend"),
                entity_in_group("be1", "BE1", "backend"),
                entity_in_group("be2", "BE2", "backend"),
            ],
            vec![relation("fe1", "be1"), relation("fe2", "be2")],
            vec![g1, g2],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        assert_eq!(result.nodes.len(), 4);
        assert!(result.groups.contains_key("frontend"));
        assert!(result.groups.contains_key("backend"));

        // Frontend 组应该在 Backend 组上方
        let fe_y = result.groups["frontend"].y;
        let be_y = result.groups["backend"].y;
        assert!(fe_y < be_y, "Frontend should be above Backend");
        assert!(
            (result.groups["frontend"].x - result.groups["backend"].x).abs() < 1.0,
            "top-level groups should be left-aligned"
        );
    }

    #[test]
    fn arch_v2_layout_microservices_groups_no_overlap() {
        let span = Span::dummy();
        let make_entity = |id: &str, label: &str, group: Option<&str>, etype: &str| -> Entity {
            let mut attrs = AttributeMap::default();
            attrs.standard.insert("type".to_string(), AttributeValue::String(TextValue::unquoted(etype.to_string())));
            Entity {
                id: Identifier::new_unchecked(id),
                label: label.to_string(),
                attributes: attrs,
                group_id: group.map(Identifier::new_unchecked),
                span,
            }
        };
        let make_rel = |from: &str, to: &str, arrow: ArrowType| -> Relation {
            Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }
        };

        let d = Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities: vec![
                make_entity("web", "Web 客户端", Some("frontend"), "frontend"),
                make_entity("mobile", "移动客户端", Some("frontend"), "frontend"),
                make_entity("gateway", "API 网关", Some("backend"), "gateway"),
                make_entity("user_svc", "用户服务", Some("backend"), "service"),
                make_entity("order_svc", "订单服务", Some("backend"), "service"),
                make_entity("db", "PostgreSQL", None, "database"),
                make_entity("mq", "消息队列", None, "queue"),
            ],
            relations: vec![
                make_rel("web", "gateway", ArrowType::Active),
                make_rel("mobile", "gateway", ArrowType::Active),
                make_rel("gateway", "user_svc", ArrowType::Active),
                make_rel("gateway", "order_svc", ArrowType::Active),
                make_rel("user_svc", "db", ArrowType::Active),
                make_rel("order_svc", "db", ArrowType::Active),
                make_rel("order_svc", "mq", ArrowType::Active),
                make_rel("mq", "user_svc", ArrowType::Passive),
            ],
            groups: vec![
                make_group("frontend", "前端层", vec!["web", "mobile"]),
                make_group("backend", "后端层", vec!["gateway", "user_svc", "order_svc"]),
            ],
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };

        let result = ArchitectureV2Layout::default().compute(&d);
        let fe = result.groups.get("frontend").unwrap();
        let be = result.groups.get("backend").unwrap();
        assert!(
            fe.y + fe.height + 8.0 <= be.y,
            "frontend bottom {:.1} should be at least 8px above backend top {:.1}",
            fe.y + fe.height,
            be.y
        );

        // P1: 被动边不参与分层，用户服务应与订单服务同层
        let user = result.nodes.get("user_svc").unwrap();
        let order = result.nodes.get("order_svc").unwrap();
        assert!(
            (user.y - order.y).abs() < 1.0,
            "user_svc and order_svc should share a layer (y {:.1} vs {:.1})",
            user.y,
            order.y
        );

        // P1: 宏观分层后后端组应紧凑（不再被 mq 回边拉散）
        assert!(
            be.height < 300.0,
            "backend group should be compact, got height {:.1}",
            be.height
        );

        // P2b: 基础设施行（mq/db）应以服务层为锚点居中
        let mq = result.nodes.get("mq").unwrap();
        let db = result.nodes.get("db").unwrap();
        let order = result.nodes.get("order_svc").unwrap();
        let user = result.nodes.get("user_svc").unwrap();
        let services_center = (order.x + order.width / 2.0 + user.x + user.width / 2.0) / 2.0;
        let infra_center = (mq.x + mq.width / 2.0 + db.x + db.width / 2.0) / 2.0;
        assert!(
            (infra_center - services_center).abs() < 80.0,
            "infra row should center near services (infra={:.1}, services={:.1})",
            infra_center,
            services_center
        );

        // P2c: 顶层分组左缘对齐
        assert!(
            (fe.x - be.x).abs() < 1.0,
            "frontend/backend groups should share left edge (fe.x={:.1}, be.x={:.1})",
            fe.x,
            be.x
        );

        // P2e: 组内 hub（gateway）水平居中于服务层
        let gateway = result.nodes.get("gateway").unwrap();
        let web = result.nodes.get("web").unwrap();
        let mobile = result.nodes.get("mobile").unwrap();
        let gw_cx = gateway.x + gateway.width / 2.0;
        let services_center = (order.x + order.width / 2.0 + user.x + user.width / 2.0) / 2.0;
        assert!(
            (gw_cx - services_center).abs() < 40.0,
            "gateway should center over services (gw={:.1}, services={:.1})",
            gw_cx,
            services_center
        );

        // P2e: 前端节点绕 gateway 对称分布，避免单侧大幅 Z 形折线
        let web_cx = web.x + web.width / 2.0;
        let mobile_cx = mobile.x + mobile.width / 2.0;
        let max_client_offset = NODE_GAP + web.width;
        assert!(
            (web_cx - gw_cx).abs() < max_client_offset + 8.0,
            "web should stay near gateway column (web={:.1}, gw={:.1})",
            web_cx,
            gw_cx
        );
        assert!(
            (mobile_cx - gw_cx).abs() < max_client_offset + 8.0,
            "mobile should stay near gateway column (mobile={:.1}, gw={:.1})",
            mobile_cx,
            gw_cx
        );

        // 所有组成员节点应落在分组包围框内（含 padding）
        for eid in ["web", "mobile"] {
            let n = result.nodes.get(eid).unwrap();
            assert!(
                n.x >= fe.x + constants::ARCH_V2_GROUP_PADDING - 0.5
                    && n.x + n.width <= fe.x + fe.width - constants::ARCH_V2_GROUP_PADDING + 0.5
                    && n.y >= fe.y + GROUP_LABEL_HEIGHT + constants::ARCH_V2_GROUP_PADDING - 0.5
                    && n.y + n.height <= fe.y + fe.height - constants::ARCH_V2_GROUP_PADDING + 0.5,
                "{eid} should stay inside frontend group"
            );
        }
        for eid in ["gateway", "user_svc", "order_svc"] {
            let n = result.nodes.get(eid).unwrap();
            assert!(
                n.x >= be.x + constants::ARCH_V2_GROUP_PADDING - 0.5
                    && n.x + n.width <= be.x + be.width - constants::ARCH_V2_GROUP_PADDING + 0.5
                    && n.y >= be.y + GROUP_LABEL_HEIGHT + constants::ARCH_V2_GROUP_PADDING - 0.5
                    && n.y + n.height <= be.y + be.height - constants::ARCH_V2_GROUP_PADDING + 0.5,
                "{eid} should stay inside backend group"
            );
        }
    }

    #[test]
    fn arch_v2_passive_edge_excluded_from_layout_graph() {
        let d = diagram(
            vec![entity("a", "A"), entity("b", "B"), entity("c", "C")],
            vec![
                Relation {
                    from: Identifier::new_unchecked("a"),
                    to: Identifier::new_unchecked("b"),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span: Span::dummy(),
                },
                Relation {
                    from: Identifier::new_unchecked("c"),
                    to: Identifier::new_unchecked("b"),
                    arrow: ArrowType::Passive,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span: Span::dummy(),
                },
                Relation {
                    from: Identifier::new_unchecked("b"),
                    to: Identifier::new_unchecked("c"),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span: Span::dummy(),
                },
            ],
            vec![],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        // 无被动边时 b 会被 c->b 拉到 c 下方；排除被动边后 b 紧跟 a
        assert!(
            result.nodes["b"].y < result.nodes["c"].y,
            "passive c->b must not pull b below c"
        );
    }

    #[test]
    fn arch_v2_layout_group_members_close() {
        let g = make_group("platform", "Platform", vec!["svc1", "svc2", "svc3"]);

        let d = diagram(
            vec![
                entity_in_group("svc1", "S1", "platform"),
                entity_in_group("svc2", "S2", "platform"),
                entity_in_group("svc3", "S3", "platform"),
            ],
            vec![relation("svc1", "svc2"), relation("svc2", "svc3")],
            vec![g],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        let group_layout = result.groups.get("platform").unwrap();

        // 所有组内节点应在组边界内
        for nid in &["svc1", "svc2", "svc3"] {
            let nl = result.nodes.get(*nid).unwrap();
            assert!(
                nl.x + nl.width >= group_layout.x - 1.0,
                "Node {} should be within group x bounds",
                nid
            );
            assert!(
                nl.x <= group_layout.x + group_layout.width + 1.0,
                "Node {} should be within group x bounds",
                nid
            );
        }
    }

    #[test]
    fn arch_v2_layout_no_overlap() {
        let d = diagram(
            vec![
                entity("a", "Alpha"),
                entity("b", "Beta"),
                entity("c", "Gamma"),
                entity("d", "Delta"),
            ],
            vec![relation("a", "b"), relation("a", "c"), relation("b", "d"), relation("c", "d")],
            vec![],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        let nodes: Vec<&NodeLayout> = result.nodes.values().collect();

        // 检查没有节点重叠
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = nodes[i];
                let b = nodes[j];
                let overlap_x = (a.x + a.width > b.x + 0.5) && (b.x + b.width > a.x + 0.5);
                let overlap_y = (a.y + a.height > b.y + 0.5) && (b.y + b.height > a.y + 0.5);
                assert!(
                    !(overlap_x && overlap_y),
                    "Nodes should not overlap"
                );
            }
        }
    }

    #[test]
    fn arch_v2_layout_empty_diagram() {
        let d = diagram(vec![], vec![], vec![]);
        let result = ArchitectureV2Layout::default().compute(&d);
        assert!(result.nodes.is_empty());
    }

    #[test]
    fn arch_v2_layout_single_node() {
        let d = diagram(vec![entity("a", "A")], vec![], vec![]);
        let result = ArchitectureV2Layout::default().compute(&d);
        assert_eq!(result.nodes.len(), 1);
    }

    #[test]
    fn debug_microservices_layout() {
        let span = Span::dummy();
        let make_entity = |id: &str, label: &str, group: Option<&str>, etype: &str| -> Entity {
            let mut attrs = AttributeMap::default();
            attrs.standard.insert("type".to_string(), AttributeValue::String(TextValue::unquoted(etype.to_string())));
            Entity {
                id: Identifier::new_unchecked(id),
                label: label.to_string(),
                attributes: attrs,
                group_id: group.map(Identifier::new_unchecked),
                span,
            }
        };
        let make_rel = |from: &str, to: &str| -> Relation {
            Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }
        };
        let make_group = |id: &str, label: &str, members: Vec<&str>| -> Group {
            Group {
                id: Identifier::new_unchecked(id),
                label: label.to_string(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: members.into_iter().map(|s| Identifier::new_unchecked(s)).collect(),
                child_group_ids: vec![],
                span,
            }
        };

        let d = Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities: vec![
                make_entity("web", "Web 客户端", Some("frontend"), "frontend"),
                make_entity("mobile", "移动客户端", Some("frontend"), "frontend"),
                make_entity("gateway", "API 网关", Some("backend"), "gateway"),
                make_entity("user_svc", "用户服务", Some("backend"), "service"),
                make_entity("order_svc", "订单服务", Some("backend"), "service"),
                make_entity("db", "PostgreSQL", None, "database"),
                make_entity("mq", "消息队列", None, "queue"),
            ],
            relations: vec![
                make_rel("web", "gateway"),
                make_rel("mobile", "gateway"),
                make_rel("gateway", "user_svc"),
                make_rel("gateway", "order_svc"),
                make_rel("user_svc", "db"),
                make_rel("order_svc", "db"),
                make_rel("order_svc", "mq"),
                make_rel("mq", "user_svc"),
            ],
            groups: vec![
                make_group("frontend", "前端层", vec!["web", "mobile"]),
                make_group("backend", "后端层", vec!["gateway", "user_svc", "order_svc"]),
            ],
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };

        let result = ArchitectureV2Layout::default().compute(&d);
        eprintln!("Total: {}x{}", result.total_width, result.total_height);
        let mut nodes: Vec<_> = result.nodes.iter().collect();
        nodes.sort_by(|a, b| {
            a.1.y.partial_cmp(&b.1.y).unwrap().then(a.1.x.partial_cmp(&b.1.x).unwrap())
        });
        for (id, n) in nodes {
            eprintln!("  {}: x={:.1} y={:.1} w={:.1} h={:.1}", id, n.x, n.y, n.width, n.height);
        }
    }

    #[test]
    fn arch_v2_layout_cyclic_graph() {
        let d = diagram(
            vec![entity("a", "A"), entity("b", "B"), entity("c", "C")],
            vec![relation("a", "b"), relation("b", "c"), relation("c", "a")],
            vec![],
        );
        let result = ArchitectureV2Layout::default().compute(&d);
        assert_eq!(result.nodes.len(), 3);
        // 不应崩溃，且不应有重叠
        let nodes: Vec<&NodeLayout> = result.nodes.values().collect();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = nodes[i];
                let b = nodes[j];
                let overlap_x = (a.x + a.width > b.x + 0.5) && (b.x + b.width > a.x + 0.5);
                let overlap_y = (a.y + a.height > b.y + 0.5) && (b.y + b.height > a.y + 0.5);
                assert!(!(overlap_x && overlap_y), "No overlap in cyclic graph");
            }
        }
    }

    #[test]
    fn arch_v2_layout_cross_group_edges() {
        let g1 = make_group("client", "Client", vec!["browser", "mobile"]);
        let g2 = make_group("server", "Server", vec!["api", "worker"]);
        let g3 = make_group("data", "Data", vec!["db", "cache"]);

        let d = diagram(
            vec![
                entity_in_group("browser", "Browser", "client"),
                entity_in_group("mobile", "Mobile", "client"),
                entity_in_group("api", "API", "server"),
                entity_in_group("worker", "Worker", "server"),
                entity_in_group("db", "Database", "data"),
                entity_in_group("cache", "Cache", "data"),
            ],
            vec![
                relation("browser", "api"),
                relation("mobile", "api"),
                relation("api", "db"),
                relation("api", "cache"),
                relation("worker", "db"),
            ],
            vec![g1, g2, g3],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        assert_eq!(result.nodes.len(), 6);
        assert_eq!(result.groups.len(), 3);

        // 验证所有节点不重叠
        let nodes: Vec<&NodeLayout> = result.nodes.values().collect();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = nodes[i];
                let b = nodes[j];
                let overlap_x = (a.x + a.width > b.x + 0.5) && (b.x + b.width > a.x + 0.5);
                let overlap_y = (a.y + a.height > b.y + 0.5) && (b.y + b.height > a.y + 0.5);
                assert!(!(overlap_x && overlap_y), "No node overlap");
            }
        }

        // 验证组内节点在组边界内
        for gid in &["client", "server", "data"] {
            let gl = result.groups.get(*gid).unwrap();
            // 至少有一个组内节点
            assert!(gl.width > 0.0 && gl.height > 0.0, "Group {} has valid bounds", gid);
        }
    }

    #[test]
    fn neighbor_alignment_chain_all_centers_equal() {
        // 3-node chain with varying widths: user(w=111) → app(w=96) → db(w=132)
        // After NeighborAlignmentPhase, all three should share the same center_x.
        let mut attrs_user = AttributeMap::default();
        attrs_user.standard.insert("type".into(), AttributeValue::String(TextValue::unquoted("frontend".to_string())));
        attrs_user.standard.insert("semantic".into(), AttributeValue::String(TextValue::unquoted("user".to_string())));
        let mut attrs_app = AttributeMap::default();
        attrs_app.standard.insert("type".into(), AttributeValue::String(TextValue::unquoted("service".to_string())));
        let mut attrs_db = AttributeMap::default();
        attrs_db.standard.insert("type".into(), AttributeValue::String(TextValue::unquoted("database".to_string())));
        attrs_db.standard.insert("semantic".into(), AttributeValue::String(TextValue::unquoted("postgres".to_string())));

        let d = diagram(
            vec![
                Entity {
                    id: Identifier::new_unchecked("user"),
                    label: "用户".into(),
                    attributes: attrs_user,
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("app"),
                    label: "应用".into(),
                    attributes: attrs_app,
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("db"),
                    label: "数据存储".into(),
                    attributes: attrs_db,
                    group_id: None,
                    span: Span::dummy(),
                },
            ],
            vec![relation("user", "app"), relation("app", "db")],
            vec![],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        let user_cx = result.nodes["user"].x + result.nodes["user"].width / 2.0;
        let app_cx = result.nodes["app"].x + result.nodes["app"].width / 2.0;
        let db_cx = result.nodes["db"].x + result.nodes["db"].width / 2.0;

        assert!(
            (user_cx - app_cx).abs() < 1.0,
            "user ({user_cx}) and app ({app_cx}) should be center-aligned"
        );
        assert!(
            (app_cx - db_cx).abs() < 1.0,
            "app ({app_cx}) and db ({db_cx}) should be center-aligned"
        );
    }

    #[test]
    fn neighbor_alignment_fan_out_source_centered() {
        // fan-out: a → b, a → c. Node a should be centered above b and c.
        let d = diagram(
            vec![
                entity("a", "Source"),
                entity("b", "Left"),
                entity("c", "Right"),
            ],
            vec![relation("a", "b"), relation("a", "c")],
            vec![],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        let a_cx = result.nodes["a"].x + result.nodes["a"].width / 2.0;
        let b_cx = result.nodes["b"].x + result.nodes["b"].width / 2.0;
        let c_cx = result.nodes["c"].x + result.nodes["c"].width / 2.0;
        let bc_mid = (b_cx + c_cx) / 2.0;

        assert!(
            (a_cx - bc_mid).abs() < 2.0,
            "source a ({a_cx}) should be centered above b ({b_cx}) and c ({c_cx}), mid={bc_mid}"
        );
    }

    #[test]
    fn neighbor_alignment_no_overlap_introduced() {
        // Multi-node layer should not overlap after alignment
        let d = diagram(
            vec![
                entity("a", "A"),
                entity("b", "B"),
                entity("c", "C"),
                entity("d", "D"),
            ],
            vec![relation("a", "c"), relation("b", "d")],
            vec![],
        );

        let result = ArchitectureV2Layout::default().compute(&d);
        let nodes: Vec<_> = result.nodes.values().collect();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = nodes[i];
                let b = nodes[j];
                let overlap_x = (a.x + a.width > b.x + 0.5) && (b.x + b.width > a.x + 0.5);
                let overlap_y = (a.y + a.height > b.y + 0.5) && (b.y + b.height > a.y + 0.5);
                assert!(!(overlap_x && overlap_y), "No node overlap after alignment");
            }
        }
    }

    #[test]
    fn super_graph_cycle_does_not_collapse_sink_to_rank_0() {
        // 3 个 group 形成超级图环：
        //   A → B (node a1 → b1)
        //   B → C (node b2 → c1)
        //   C → A (node c2 → a2)
        // 无 FAS 去环时，C 的 sink 节点可能被错误分配到 rank 0。
        // 有 FAS 去环后，应产生合理的层级（A → B → C 或类似）。
        let g_a = make_group("ga", "GA", vec!["a1", "a2"]);
        let g_b = make_group("gb", "GB", vec!["b1", "b2"]);
        let g_c = make_group("gc", "GC", vec!["c1", "c2"]);

        let d = diagram(
            vec![
                entity_in_group("a1", "A1", "ga"),
                entity_in_group("a2", "A2", "ga"),
                entity_in_group("b1", "B1", "gb"),
                entity_in_group("b2", "B2", "gb"),
                entity_in_group("c1", "C1", "gc"),
                entity_in_group("c2", "C2", "gc"),
            ],
            vec![
                relation("a1", "b1"), // ga → gb
                relation("b2", "c1"), // gb → gc
                relation("c2", "a2"), // gc → ga (back edge, forms cycle)
            ],
            vec![g_a, g_b, g_c],
        );

        let result = ArchitectureV2Layout::default().compute(&d);

        // 验证：三个 group 不应全在同一个 y（rank 0）
        let y_a = result.groups["ga"].y;
        let y_b = result.groups["gb"].y;
        let y_c = result.groups["gc"].y;

        // 至少有两个 group 在不同 y 行
        let distinct_y = {
            let mut ys = vec![y_a, y_b, y_c];
            ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
            ys.len()
        };
        assert!(
            distinct_y >= 2,
            "groups should not all collapse to same rank: y_a={}, y_b={}, y_c={}",
            y_a, y_b, y_c
        );

        // 验证：group 之间不应重叠
        let groups: Vec<_> = ["ga", "gb", "gc"]
            .iter()
            .filter_map(|id| result.groups.get(*id))
            .collect();
        for i in 0..groups.len() {
            for j in (i + 1)..groups.len() {
                let a = groups[i];
                let b = groups[j];
                let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                assert!(
                    x_overlap <= 1.0 || y_overlap <= 1.0,
                    "groups {} and {} overlap",
                    i, j
                );
            }
        }
    }
