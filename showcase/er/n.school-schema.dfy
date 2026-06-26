// 学校管理系统 ER
// Mermaid 对照: erDiagram 多对多关系
diagram er {
    title: "学校管理系统"

    entity student "Student" {
        type: database
        meta.pk: "id"
        meta.fields: "name\ngrade"
    }
    entity course "Course" {
        type: database
        meta.pk: "id"
        meta.fields: "title\ncredits"
    }
    entity teacher "Teacher" {
        type: database
        meta.pk: "id"
        meta.fields: "name\ndepartment"
    }
    entity enrollment "Enrollment" {
        type: database
        meta.pk: "id"
        meta.fields: "fk.student_id\nfk.course_id\nenrolled_at"
    }
    entity classroom "Classroom" {
        type: database
        meta.pk: "id"
        meta.fields: "building\nroom_no"
    }

    student -> enrollment "选课" { cardinality: "1:N" }
    course -> enrollment "被选" { cardinality: "1:N" }
    teacher -> course "授课" { cardinality: "1:N" }
    classroom -> course "安排" { cardinality: "1:N" }
}
